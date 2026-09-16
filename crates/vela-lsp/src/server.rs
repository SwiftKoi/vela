//! The server: a loop, a session, and the diagnostics both already exist.
//!
//! # What is served, and what is deliberately not
//!
//! Diagnostics and document sync — `publishDiagnostics`, which is what the parity criterion in
//! `M10-tooling.md` is about, and which is most of what an editor needs to be useful. Hover,
//! completion, goto-definition, references, and rename are *not* declared: `initialize` advertises a
//! capability only once it answers, because an editor told a capability exists will call it, and a
//! capability that answers nothing is worse than one that is absent — the editor stops looking, and
//! the feature reads as broken rather than as missing.
//!
//! # Why the session is handed over rather than built here
//!
//! A session is a project's sources, its entry point, and its asset manifest, and two of those come
//! from a `vela.toml` this crate cannot read — the protocol knows nothing about projects, and the
//! command line already knows everything. So the caller passes a loader, and this crate stays what it
//! claims to be: an adapter that decides nothing about what is wrong with a program.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};

use serde_json::{Value, json};
use vela_compile::Session;
use vela_diag::{Diagnostic, Severity};

use crate::diagnostics;
use crate::position;
use crate::transport;
use crate::uri;

/// Builds a session for the workspace rooted at a path.
///
/// The command line implements this with the loader `vela check` uses, which is what keeps the editor
/// and the command in agreement: one function decides what a project's files are.
type Loader = Box<dyn Fn(&str) -> Session>;

/// One language server.
pub struct Server {
    /// How a workspace becomes a session.
    load: Loader,
    /// The workspace: its files, and everything the compiler has memoized about them.
    session: Session,
    /// The root, as the client named it or as the process was started.
    root: String,
    /// Open documents: URI to the name the session knows the file by.
    open: BTreeMap<String, String>,
}

impl Server {
    /// A server for the workspace at `root`, loaded by `load`.
    #[must_use]
    pub fn new(root: impl Into<String>, load: impl Fn(&str) -> Session + 'static) -> Self {
        let root = root.into();
        let session = load(&root);

        Self {
            load: Box::new(load),
            session,
            root,
            open: BTreeMap::new(),
        }
    }

    /// Answers messages until the client exits or the stream ends.
    ///
    /// # Errors
    ///
    /// Fails on a transport error, which is not recoverable: a stream whose framing is broken has no
    /// next message to resynchronise to.
    pub fn serve(
        &mut self,
        input: &mut impl BufRead,
        // `dyn` rather than `impl`: the caller is a subcommand, and a subcommand is handed
        // `&mut dyn Write` by the driver. Generic here would only move the same coercion to the call
        // site, and there is nothing to inline.
        output: &mut dyn Write,
    ) -> std::io::Result<()> {
        while let Some(message) = transport::read(input)? {
            if !self.handle(&message, output)? {
                break;
            }
        }
        Ok(())
    }

    /// Answers one message, reporting whether to keep going.
    fn handle(&mut self, message: &Value, output: &mut dyn Write) -> std::io::Result<bool> {
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        let id = message.get("id").cloned();

        match method {
            "initialize" => {
                self.adopt_root(message);
                self.respond(output, id, self.capabilities())?;
            }
            "initialized" => {}
            "shutdown" => self.respond(output, id, Value::Null)?,
            "exit" => return Ok(false),
            "textDocument/didOpen" => {
                let document = &message["params"]["textDocument"];
                let uri = document["uri"].as_str().unwrap_or("").to_string();
                self.set_document(&uri, document["text"].as_str().unwrap_or(""));
                self.publish(&uri, output)?;
            }
            "textDocument/didChange" => {
                let uri = string_at(message, &["params", "textDocument", "uri"]);
                // The last change wins under full sync, which is what `textDocumentSync: 1` promises.
                let text = message["params"]["contentChanges"]
                    .as_array()
                    .and_then(|changes| changes.last())
                    .and_then(|change| change["text"].as_str())
                    .unwrap_or("");
                self.set_document(&uri, text);
                self.publish(&uri, output)?;
            }
            "textDocument/didClose" => {
                let uri = string_at(message, &["params", "textDocument", "uri"]);
                self.open.remove(&uri);
                // A closed document has no diagnostics: an editor keeps the last set until it is told
                // otherwise, and a red squiggle in a file nobody has open is a bug report about nothing.
                self.publish_items(&uri, Vec::new(), output)?;
            }
            // A request for something this server does not do is answered as *not found*, which is the
            // truthful answer: the capability was not advertised, and saying so is better than silence,
            // which an editor waits on until it times out.
            _ => {
                if id.is_some() {
                    self.respond_error(output, id, -32601, &format!("no `{method}` here"))?;
                }
            }
        }
        Ok(true)
    }

    /// What this server can do.
    fn capabilities(&self) -> Value {
        json!({
            "capabilities": {
                // Full sync: a change carries the whole document. Incremental sync is an optimisation
                // for large files, and building it before the features that need positions exist would
                // be work in the wrong order.
                "textDocumentSync": 1,
                // Stated rather than left to the default, because it is a claim about every range this
                // server ever sends (`crate::position`).
                "positionEncoding": "utf-16",
            },
            "serverInfo": { "name": "vela-lsp", "version": env!("CARGO_PKG_VERSION") },
        })
    }

    /// Adopts the root the client names, reloading when it is not the one we started with.
    ///
    /// An editor launched anywhere can open a project anywhere, so the root the client sends is the one
    /// that counts — but reloading on every `initialize` would throw away the memoized results of a
    /// server that was started in the right place, so it happens only when the answer differs.
    fn adopt_root(&mut self, message: &Value) {
        let named = message["params"]["rootUri"]
            .as_str()
            .and_then(uri::path)
            .or_else(|| {
                message["params"]["rootPath"]
                    .as_str()
                    .map(ToString::to_string)
            });

        if let Some(root) = named
            && root != self.root
        {
            self.session = (self.load)(&root);
            self.root = root;
            self.open.clear();
        }
    }

    /// Records a document's text, keeping the session's identity for it.
    ///
    /// `Session::set_file` replaces the text of a name it already knows and keeps the file's id, which
    /// is what lets an edit invalidate one module rather than everything.
    fn set_document(&mut self, uri: &str, text: &str) {
        let name = self.name_of(uri);
        self.session.set_file(name.clone(), text.to_string());
        self.open.insert(uri.to_string(), name);
    }

    /// The name the session knows a document by.
    fn name_of(&self, uri: &str) -> String {
        self.open.get(uri).cloned().unwrap_or_else(|| {
            uri::path(uri).map_or_else(|| uri.to_string(), |path| uri::name(&self.root, &path))
        })
    }

    /// Publishes a document's diagnostics.
    fn publish(&mut self, uri: &str, output: &mut dyn Write) -> std::io::Result<()> {
        let Some(name) = self.open.get(uri).cloned() else {
            return Ok(());
        };
        let Some(file) = self.session.file_named(&name) else {
            return Ok(());
        };

        let items: Vec<Value> = diagnostics::file(&mut self.session, file)
            .iter()
            .map(|diagnostic| self.item(diagnostic))
            .collect();
        self.publish_items(uri, items, output)
    }

    /// Sends one `publishDiagnostics`.
    fn publish_items(
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

    /// One diagnostic, in the shape the protocol wants.
    fn item(&self, diagnostic: &Diagnostic) -> Value {
        let range = position::range(self.session.sources(), diagnostic.primary.span);

        // The headline and the label, joined: an editor shows one message, and this compiler writes its
        // diagnostics as a sentence with the detail in the label ("this path reaches the end without
        // returning"). Dropping the label would lose the half that says which part is meant.
        let message = if diagnostic.primary.message.is_empty() {
            diagnostic.message.clone()
        } else {
            format!("{} — {}", diagnostic.message, diagnostic.primary.message)
        };

        json!({
            "range": {
                "start": { "line": range.start.line, "character": range.start.character },
                "end": { "line": range.end.line, "character": range.end.character },
            },
            // The protocol's own numbering. A lint is an *information* message rather than a hint:
            // this compiler calls it "a style or hygiene suggestion" and `vela check` counts it
            // alongside warnings, so an editor that hid it behind a hint icon would show less than the
            // command line does.
            "severity": match diagnostic.severity() {
                Severity::Error => 1,
                Severity::Warning => 2,
                Severity::Lint => 3,
            },
            "code": diagnostic.code.as_str(),
            "source": "vela",
            "message": message,
        })
    }

    /// Answers a request.
    fn respond(
        &self,
        output: &mut dyn Write,
        id: Option<Value>,
        result: Value,
    ) -> std::io::Result<()> {
        let Some(id) = id else {
            return Ok(());
        };
        transport::write(
            output,
            &json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        )
    }

    /// Answers a request with an error.
    fn respond_error(
        &self,
        output: &mut dyn Write,
        id: Option<Value>,
        code: i32,
        message: &str,
    ) -> std::io::Result<()> {
        let Some(id) = id else {
            return Ok(());
        };
        transport::write(
            output,
            &json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": code, "message": message },
            }),
        )
    }
}

/// A string at a path within a message, or the empty string.
fn string_at(message: &Value, path: &[&str]) -> String {
    let mut value = message;
    for step in path {
        value = &value[*step];
    }
    value.as_str().unwrap_or("").to_string()
}
