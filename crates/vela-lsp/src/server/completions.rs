//! Answering `textDocument/completion`.

use serde_json::{Value, json};

use super::state::Server;
use crate::completion;

impl Server {
    /// What could be typed at a position, in the shape the protocol wants.
    ///
    /// An empty list rather than `null` when the position is not in an open document: completion is
    /// asked constantly, and a malformed answer is worse than an empty one.
    pub(super) fn completions(&mut self, message: &Value) -> Value {
        let Some((file, offset)) = self.position_of(message) else {
            return json!({ "isIncomplete": false, "items": [] });
        };

        let items: Vec<Value> = completion::at(&mut self.session, file, offset)
            .into_iter()
            .map(|item| json!({ "label": item.label, "detail": item.detail }))
            .collect();

        // `isIncomplete: false` is a promise that this list is the whole list for this position, which
        // is what lets an editor filter it locally as the author keeps typing. The lists here are short
        // — a module's labels, a body's locals — so the promise costs nothing to keep.
        json!({ "isIncomplete": false, "items": items })
    }
}
