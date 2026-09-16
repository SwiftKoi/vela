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
            "textDocument/didOpen" | "textDocument/didChange" | "textDocument/didClose" => {
                self.document_changed(method, message, output)?;
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
            "textDocument/completion" => {
                let items = self.completions(message);
                self.respond(output, id, items)?;
            }
            "textDocument/hover" => {
                let hover = self.hover(message);
                self.respond(output, id, hover)?;
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
