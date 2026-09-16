//! One message in, one answer out: which handler answers a DAP command.

use std::io::{self, Write};

use serde_json::Value;

use super::state::Server;
use crate::debuggee::Mode;

impl Server {
    /// Answers one message, reporting whether to keep going.
    ///
    /// `false` means the client is done: `disconnect` and `terminate` end the loop rather than
    /// leaving it to block on a stdin that will never be written again.
    ///
    /// # Errors
    ///
    /// Fails if the answer cannot be written. An unknown `command` is *not* an error — it is
    /// answered with a failure so the client learns what happened instead of waiting.
    pub(super) fn handle(&mut self, message: &Value, output: &mut dyn Write) -> io::Result<bool> {
        let command = message
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let request_seq = message.get("seq").and_then(Value::as_i64).unwrap_or(0);
        let arguments = message.get("arguments").cloned().unwrap_or(Value::Null);

        match command.as_str() {
            "initialize" => self.initialize(request_seq, output)?,
            "launch" | "attach" => self.launch(&arguments, request_seq, output, &command)?,
            "configurationDone" => self.configuration_done(request_seq, output)?,
            "setBreakpoints" => self.set_breakpoints(&arguments, request_seq, output)?,
            "setFunctionBreakpoints" => {
                self.set_function_breakpoints(&arguments, request_seq, output)?;
            }
            "threads" => self.threads(request_seq, output)?,
            "stackTrace" => self.stack_trace(request_seq, output)?,
            "scopes" => self.scopes(&arguments, request_seq, output)?,
            "variables" => self.variables(&arguments, request_seq, output)?,
            "evaluate" => self.evaluate(&arguments, request_seq, output)?,
            "continue" => self.resume(Mode::Continue, request_seq, output, &command)?,
            "next" => self.resume(Mode::Over, request_seq, output, &command)?,
            "stepIn" => self.resume(Mode::Into, request_seq, output, &command)?,
            "stepOut" => self.resume(Mode::Out, request_seq, output, &command)?,
            "stepBack" => self.step_back(request_seq, output, &command)?,
            // `reverseContinue` means "run backwards to the previous breakpoint". The ring can go
            // back a command at a time but does not record where the breakpoints were, so this is
            // refused rather than approximated by something that is not what was asked for.
            "reverseContinue" => self.fail(
                output,
                request_seq,
                &command,
                "reverse continue is not supported; `step back` walks one command at a time",
            )?,
            // A synchronous adapter is always stopped when it is reading, so there is nothing to
            // interrupt. Answered rather than ignored, because a client that asked deserves an
            // answer even when the answer is "already paused".
            "pause" => self.respond(output, request_seq, &command, Value::Null)?,
            "disconnect" | "terminate" => {
                self.respond(output, request_seq, &command, Value::Null)?;
                self.event(output, "terminated", serde_json::json!({}))?;
                return Ok(false);
            }
            other => self.fail(
                output,
                request_seq,
                other,
                "vela-debug does not implement this request",
            )?,
        }
        Ok(true)
    }
}
