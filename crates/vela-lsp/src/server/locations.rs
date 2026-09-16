//! The two index queries: where a name is declared, and every place it is written.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use vela_span::{FileId, Span};

use super::reply::{number_at, string_at};
use super::state::Server;
use crate::position;
use crate::symbols;
use crate::uri;

impl Server {
    /// The locations a request at a position names, for one of the two index capabilities.
    ///
    /// One helper for both because they differ in which list they read — the declaration, or every
    /// occurrence — and not in how a position becomes an offset or a span becomes a location.
    pub(super) fn locations(&mut self, message: &Value, references: bool) -> Value {
        let Some(found) = self.target(message) else {
            return Value::Null;
        };

        let spans: Vec<(FileId, Span)> = if references {
            symbols::occurrences(&mut self.session, &found.named)
        } else {
            symbols::declaration(&mut self.session, &found.named)
                .into_iter()
                .collect()
        };

        // An empty *list* is a real answer for references — "nothing else writes this name" — while a
        // definition that was not found is `null`. The protocol distinguishes the two, and so should a
        // server: one means "I looked", the other means "I have nothing to say".
        if spans.is_empty() {
            return if references { json!([]) } else { Value::Null };
        }

        let locations: Vec<Value> = spans
            .into_iter()
            .filter_map(|(file, span)| self.location(file, span))
            .collect();
        json!(locations)
    }

    /// The edit a rename would make, or `null` when it cannot be made safely.
    ///
    /// `null` covers the two ways that happens and the editor shows "cannot rename here" for both: the
    /// offset names nothing this index knows, or one of the places the name is written cannot be
    /// located precisely (`symbols::rename` refuses a partial edit).
    pub(super) fn rename(&mut self, message: &Value) -> Value {
        let Some(found) = self.target(message) else {
            return Value::Null;
        };
        let Some(spans) = symbols::rename(&mut self.session, &found.named) else {
            return Value::Null;
        };

        // Grouped by file, which is the shape a workspace edit takes: one document's changes together,
        // so an editor applies each file once and can leave files the rename did not touch alone.
        let mut changes: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for (file, span) in spans {
            let Some(uri) = self.uri_of(file) else {
                continue;
            };
            let range = position::range(self.session.sources(), span);
            changes.entry(uri).or_default().push(json!({
                "range": {
                    "start": { "line": range.start.line, "character": range.start.character },
                    "end": { "line": range.end.line, "character": range.end.character },
                },
                "newText": string_at(message, &["params", "newName"]),
            }));
        }

        if changes.is_empty() {
            return Value::Null;
        }
        json!({ "changes": changes })
    }

    /// What to say about a position, or `null` when there is nothing to say.
    ///
    /// `null` rather than an empty answer: an editor draws a hover box for a non-empty string and
    /// nothing for `null`, and a box that says nothing is worse than no box.
    pub(super) fn hover(&mut self, message: &Value) -> Value {
        let Some((file, offset)) = self.position_of(message) else {
            return Value::Null;
        };

        let Some(hover) = crate::hover::at(&mut self.session, file, offset, &self.reference) else {
            return Value::Null;
        };

        let mut result = json!({
            "contents": { "kind": "markdown", "value": hover.markdown },
        });
        // The range only when the answer is *in this document*: a label declared in another file has a
        // span, and a range in a different file's coordinates would underline unrelated characters.
        if let Some(span) = hover.range {
            let range = position::range(self.session.sources(), span);
            result["range"] = json!({
                "start": { "line": range.start.line, "character": range.start.character },
                "end": { "line": range.end.line, "character": range.end.character },
            });
        }
        result
    }

    /// The document a request is about, and the offset in it.
    pub(super) fn position_of(&mut self, message: &Value) -> Option<(FileId, u32)> {
        let uri = self.request_uri(message)?;
        let name = self.open.get(&uri).cloned()?;
        let file = self.session.file_named(&name)?;

        let at = position::Position {
            line: number_at(message, &["params", "position", "line"]),
            character: number_at(message, &["params", "position", "character"]),
        };
        let offset = position::offset(self.session.sources(), file, at)?;
        Some((file, offset))
    }

    /// The URI a request is about.
    fn request_uri(&self, message: &Value) -> Option<String> {
        let uri = string_at(message, &["params", "textDocument", "uri"]);
        (!uri.is_empty()).then_some(uri)
    }

    /// What the offset in a request's position names, if anything.
    ///
    /// Shared by the three index capabilities: each of them starts by asking what the editor is pointing
    /// at, and only what they do with the answer differs.
    pub(super) fn target(&mut self, message: &Value) -> Option<symbols::Found> {
        let (file, offset) = self.position_of(message)?;
        symbols::at(&mut self.session, file, offset)
    }

    /// The URI of a file, addressed the way the editor addresses it.
    fn uri_of(&self, file: FileId) -> Option<String> {
        let name = self.session.sources().get(file)?.name().to_string();
        Some(uri::of(&self.source, &name))
    }

    /// One location: a file, addressed the way the editor addresses it, and a range.
    ///
    /// The URI is rebuilt from the source root rather than remembered from `didOpen`, because
    /// goto-definition lands in files nobody has opened — which is the whole point of it.
    fn location(&self, file: FileId, span: Span) -> Option<Value> {
        let uri = self.uri_of(file)?;
        let range = position::range(self.session.sources(), span);

        Some(json!({
            "uri": uri,
            "range": {
                "start": { "line": range.start.line, "character": range.start.character },
                "end": { "line": range.end.line, "character": range.end.character },
            },
        }))
    }
}
