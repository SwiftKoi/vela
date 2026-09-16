//! One message out: responses, events, and the sequence numbers that tie them together.

use std::io::{self, Write};

use serde_json::{Value, json};

use super::state::{Scope, Server};
use crate::transport;

impl Server {
    /// Answers a request successfully.
    pub(super) fn respond(
        &mut self,
        output: &mut dyn Write,
        request_seq: i64,
        command: &str,
        body: Value,
    ) -> io::Result<()> {
        let seq = self.sequence();
        self.send(
            output,
            json!({
                "seq": seq,
                "type": "response",
                "request_seq": request_seq,
                "success": true,
                "command": command,
                "body": body,
            }),
        )
    }

    /// Answers a request with a failure.
    ///
    /// The message is the whole answer for a refused capability, and for an expression the
    /// checker rejected it is the `Exxx` itself: a debugger's error is the compiler's error, not
    /// an evaluator's crash (`TOOLING.md §6`).
    pub(super) fn fail(
        &mut self,
        output: &mut dyn Write,
        request_seq: i64,
        command: &str,
        message: &str,
    ) -> io::Result<()> {
        let seq = self.sequence();
        self.send(
            output,
            json!({
                "seq": seq,
                "type": "response",
                "request_seq": request_seq,
                "success": false,
                "command": command,
                "message": message,
            }),
        )
    }

    /// Sends an event.
    pub(super) fn event(
        &mut self,
        output: &mut dyn Write,
        event: &str,
        body: Value,
    ) -> io::Result<()> {
        let seq = self.sequence();
        self.send(
            output,
            json!({ "seq": seq, "type": "event", "event": event, "body": body }),
        )
    }

    /// Allocates a `variables` reference for a scope.
    pub(super) fn allocate(&mut self, scope: Scope) -> i64 {
        self.next_reference += 1;
        self.scopes.insert(self.next_reference, scope);
        self.next_reference
    }

    /// The next outgoing sequence number.
    fn sequence(&mut self) -> i64 {
        self.next_seq += 1;
        self.next_seq
    }

    /// Writes one message, framed.
    fn send(&mut self, output: &mut dyn Write, message: Value) -> io::Result<()> {
        transport::write(output, &message)
    }
}
