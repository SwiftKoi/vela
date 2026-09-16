//! One message in, one message out: which handler answers it.

use std::io::Write;

use serde_json::Value;

use super::reply::string_at;
use super::state::Server;

impl Server {
    /// Answers one message, reporting whether to keep going.
    pub(super) fn handle(
        &mut self,
        message: &Value,
        output: &mut dyn Write,
    ) -> std::io::Result<bool> {
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
            // The two features that read the symbol index: where a name is declared, and every place it
            // is written. Same question, so one helper answers both.
            "textDocument/definition" => {
                let locations = self.locations(message, false);
                self.respond(output, id, locations)?;
            }
            "textDocument/references" => {
                let locations = self.locations(message, true);
                self.respond(output, id, locations)?;
            }
            "textDocument/rename" => {
                let new_name = string_at(message, &["params", "newName"]);
                // Checked here rather than left to the checker afterwards: a rename that writes a name
                // the language cannot read back would leave the file unparseable, and the author would
                // be looking at a syntax error they did not write.
                if !vela_syntax::is_name(&new_name) {
                    self.respond_error(output, id, -32602, &format!("`{new_name}` is not a name"))?;
                } else {
                    let edit = self.rename(message);
                    self.respond(output, id, edit)?;
                }
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
}
