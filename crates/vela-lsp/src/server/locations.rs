//! The two index queries: where a name is declared, and every place it is written.

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
        let uri = string_at(message, &["params", "textDocument", "uri"]);
        let Some(name) = self.open.get(&uri).cloned() else {
            return Value::Null;
        };
        let Some(file) = self.session.file_named(&name) else {
            return Value::Null;
        };

        let at = position::Position {
            line: number_at(message, &["params", "position", "line"]),
            character: number_at(message, &["params", "position", "character"]),
        };
        let Some(offset) = position::offset(self.session.sources(), file, at) else {
            return Value::Null;
        };
        let Some(found) = symbols::at(&mut self.session, file, offset) else {
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

    /// One location: a file, addressed the way the editor addresses it, and a range.
    ///
    /// The URI is rebuilt from the source root rather than remembered from `didOpen`, because
    /// goto-definition lands in files nobody has opened — which is the whole point of it.
    fn location(&self, file: FileId, span: Span) -> Option<Value> {
        let sources = self.session.sources();
        let name = sources.get(file)?.name().to_string();
        let range = position::range(sources, span);

        Some(json!({
            "uri": uri::of(&self.source, &name),
            "range": {
                "start": { "line": range.start.line, "character": range.start.character },
                "end": { "line": range.end.line, "character": range.end.character },
            },
        }))
    }
}
