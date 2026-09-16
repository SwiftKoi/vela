//! The documents a client has open, and the diagnostics of each.

use std::io::Write;

use serde_json::{Value, json};
use vela_compile::Session;
use vela_diag::{Diagnostic, Severity};

use super::state::Server;
use crate::diagnostics;
use crate::position;
use crate::transport;
use crate::uri;

impl Server {
    /// Records a document's text, keeping the session's identity for it.
    ///
    /// `Session::set_file` replaces the text of a name it already knows and keeps the file's id, which
    /// is what lets an edit invalidate one module rather than everything.
    pub(super) fn set_document(&mut self, uri: &str, text: &str) {
        let name = self.name_of(uri);
        self.session.set_file(name.clone(), text.to_string());
        self.open.insert(uri.to_string(), name);
    }

    /// The name the session knows a document by.
    pub(super) fn name_of(&self, uri: &str) -> String {
        self.open.get(uri).cloned().unwrap_or_else(|| {
            uri::path(uri).map_or_else(|| uri.to_string(), |path| uri::name(&self.source, &path))
        })
    }

    /// Publishes a document's diagnostics.
    pub(super) fn publish(&mut self, uri: &str, output: &mut dyn Write) -> std::io::Result<()> {
        let Some(name) = self.open.get(uri).cloned() else {
            return Ok(());
        };
        let Some(file) = self.session.file_named(&name) else {
            return Ok(());
        };

        // The list first, then the rendering: asking for the diagnostics borrows the session mutably,
        // and a range needs the same session's sources to become a position.
        let found = diagnostics::file(&mut self.session, file);
        let items: Vec<Value> = found
            .iter()
            .map(|diagnostic| item(&self.session, diagnostic))
            .collect();
        self.publish_items(uri, items, output)
    }

    /// Sends one `publishDiagnostics`.
    pub(super) fn publish_items(
        &self,
        uri: &str,
        diagnostics: Vec<Value>,
        output: &mut dyn Write,
    ) -> std::io::Result<()> {
        transport::write(
            output,
            &json!({
                "jsonrpc": "2.0",
                "method": "textDocument/publishDiagnostics",
                "params": { "uri": uri, "diagnostics": diagnostics },
            }),
        )
    }
}

/// One diagnostic, in the shape the protocol wants.
///
/// A free function rather than a method, because it needs the session only for the source map and the
/// caller is in the middle of borrowing it.
fn item(session: &Session, diagnostic: &Diagnostic) -> Value {
    let range = position::range(session.sources(), diagnostic.primary.span);

    json!({
        "range": {
            "start": { "line": range.start.line, "character": range.start.character },
            "end": { "line": range.end.line, "character": range.end.character },
        },
        "severity": severity_number(diagnostic.severity()),
        "code": diagnostic.code.as_str(),
        "source": "vela",
        "message": message(diagnostic),
    })
}

/// The protocol's numbering.
///
/// A lint is an *information* message rather than a hint: this compiler calls it "a style or hygiene
/// suggestion" and `vela check` counts it alongside warnings, so an editor that hid it behind a hint
/// icon would show less than the command line does.
fn severity_number(severity: Severity) -> u8 {
    match severity {
        Severity::Error => 1,
        Severity::Warning => 2,
        Severity::Lint => 3,
    }
}

/// The headline and the label, joined: an editor shows one message, and this compiler writes its
/// diagnostics as a sentence with the detail in the label ("this path reaches the end without
/// returning"). Dropping the label would lose the half that says which part is meant.
fn message(diagnostic: &Diagnostic) -> String {
    if diagnostic.primary.message.is_empty() {
        diagnostic.message.clone()
    } else {
        format!("{} — {}", diagnostic.message, diagnostic.primary.message)
    }
}
