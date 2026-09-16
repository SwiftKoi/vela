//! The handshake, breakpoints, and running the story: the requests that move it.

use std::io::{self, Write};

use serde_json::{Value, json};

use super::state::Server;
use crate::debuggee::{Debuggee, Mode, StopReason};

impl Server {
    /// `initialize` — say what this adapter can do, then that configuration may begin.
    ///
    /// The `initialized` event is what tells a client to send its breakpoints; sending it before
    /// the response, or never, is the difference between a debug session that starts and one that
    /// hangs waiting.
    pub(super) fn initialize(
        &mut self,
        request_seq: i64,
        output: &mut dyn Write,
    ) -> io::Result<()> {
        let capabilities = self.capabilities();
        self.respond(output, request_seq, "initialize", capabilities)?;
        self.event(output, "initialized", json!({}))
    }

    /// `launch` / `attach` — record how to start, and start later.
    ///
    /// Starting here rather than on `configurationDone` would run the story before the client's
    /// breakpoints arrived, which is races the one thing the two requests exist to avoid. So this
    /// only reads its arguments.
    pub(super) fn launch(
        &mut self,
        arguments: &Value,
        request_seq: i64,
        output: &mut dyn Write,
        command: &str,
    ) -> io::Result<()> {
        self.stop_on_entry = arguments
            .get("stopOnEntry")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // A release build carries no source map (`BYTECODE.md §5`), so a line breakpoint can never
        // be honoured. Saying so once, up front, beats accepting one and silently never stopping.
        if !self.program.is_debuggable() {
            self.event(
                output,
                "output",
                json!({
                    "category": "console",
                    "output": "this build has no debug info, so line breakpoints are unavailable; \
                               label breakpoints and stepping still work\n",
                }),
            )?;
        }

        self.respond(output, request_seq, command, Value::Null)
    }

    /// `configurationDone` — start the story, and run it or stop on entry.
    pub(super) fn configuration_done(
        &mut self,
        request_seq: i64,
        output: &mut dyn Write,
    ) -> io::Result<()> {
        self.respond(output, request_seq, "configurationDone", Value::Null)?;
        self.start(output)
    }

    /// Starts the story and runs to the first stop.
    fn start(&mut self, output: &mut dyn Write) -> io::Result<()> {
        match Debuggee::new(self.program.module(), self.program.entry()) {
            Ok(debuggee) => self.debuggee = Some(debuggee),
            Err(fault) => {
                self.event(
                    output,
                    "output",
                    json!({ "category": "stderr", "output": format!("cannot start: {fault}\n") }),
                )?;
                return self.event(output, "terminated", json!({}));
            }
        }

        if self.stop_on_entry {
            self.stopped = true;
            self.report(output, StopReason::Entry)
        } else {
            self.run(output, Mode::Continue)
        }
    }

    /// `setBreakpoints` — replace the line breakpoints in one file.
    ///
    /// A breakpoint is verified when the module carries debug info *and* the line exists: an
    /// unverified breakpoint is reported as such rather than accepted, so a client can show it
    /// hollow instead of promising a stop that will never come.
    pub(super) fn set_breakpoints(
        &mut self,
        arguments: &Value,
        request_seq: i64,
        output: &mut dyn Write,
    ) -> io::Result<()> {
        let path = arguments
            .pointer("/source/path")
            .and_then(Value::as_str)
            .unwrap_or("");
        let requested = arguments
            .get("breakpoints")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let file = self.program.file_for_path(path);
        let debuggable = self.program.is_debuggable();
        let mut lines = Vec::new();
        let mut answer = Vec::new();

        for breakpoint in &requested {
            let line = breakpoint.get("line").and_then(Value::as_u64).unwrap_or(0);
            let zero = u32::try_from(line.saturating_sub(1)).unwrap_or(u32::MAX);
            let verified = debuggable
                && file
                    .and_then(|file| self.program.line_count(file))
                    .is_some_and(|count| zero < count);
            if verified {
                lines.push(zero);
            }
            answer.push(json!({ "verified": verified, "line": line }));
        }

        if let Some(file) = file {
            self.breakpoints.set_lines(file, lines);
        }
        self.respond(
            output,
            request_seq,
            "setBreakpoints",
            json!({ "breakpoints": answer }),
        )
    }

    /// `setFunctionBreakpoints` — replace the label breakpoints.
    pub(super) fn set_function_breakpoints(
        &mut self,
        arguments: &Value,
        request_seq: i64,
        output: &mut dyn Write,
    ) -> io::Result<()> {
        let requested = arguments
            .get("breakpoints")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut names = Vec::new();
        let mut answer = Vec::new();

        for breakpoint in &requested {
            let name = breakpoint
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let known = self.has_label(&name);
            if known {
                names.push(name.clone());
            }
            answer.push(json!({ "verified": known, "name": name }));
        }

        self.breakpoints.set_labels(names);
        self.respond(
            output,
            request_seq,
            "setFunctionBreakpoints",
            json!({ "breakpoints": answer }),
        )
    }

    /// Whether a label by this name is in the program, under the same rule a breakpoint matches by.
    fn has_label(&self, name: &str) -> bool {
        let module = self.program.module();
        module.labels.iter().any(|label| {
            module.strings.get(label.name).is_some_and(|label_name| {
                label_name == name || label_name.ends_with(&format!(".{name}"))
            })
        })
    }

    /// `continue` / `next` / `stepIn` / `stepOut` — answer, then run to the next stop.
    pub(super) fn resume(
        &mut self,
        mode: Mode,
        request_seq: i64,
        output: &mut dyn Write,
        command: &str,
    ) -> io::Result<()> {
        self.respond(output, request_seq, command, Value::Null)?;
        self.run(output, mode)
    }

    /// `stepBack` — one command backwards, through the rollback ring.
    pub(super) fn step_back(
        &mut self,
        request_seq: i64,
        output: &mut dyn Write,
        command: &str,
    ) -> io::Result<()> {
        self.respond(output, request_seq, command, Value::Null)?;
        let reason = match self.debuggee.as_mut() {
            Some(debuggee) => debuggee.step_back(1),
            None => return Ok(()),
        };
        self.stopped = true;
        self.scopes.clear();
        self.report(output, reason)
    }

    /// Runs the story to its next stop and reports it.
    fn run(&mut self, output: &mut dyn Write, mode: Mode) -> io::Result<()> {
        let reason = {
            // Destructured so the program and the breakpoints can be borrowed immutably while the
            // debuggee is borrowed mutably: three fields of one `self`, borrowed apart.
            let Self {
                program,
                breakpoints,
                debuggee,
                ..
            } = self;
            let Some(debuggee) = debuggee.as_mut() else {
                return Ok(());
            };
            debuggee.resume(program, breakpoints, mode)
        };

        self.scopes.clear();
        self.report(output, reason)
    }

    /// Emits what a stop means to a client.
    fn report(&mut self, output: &mut dyn Write, reason: StopReason) -> io::Result<()> {
        match &reason {
            StopReason::Terminated => {
                self.stopped = false;
                self.event(output, "terminated", json!({}))
            }
            StopReason::Fault(message) => {
                self.stopped = false;
                self.event(
                    output,
                    "output",
                    json!({ "category": "stderr", "output": format!("{message}\n") }),
                )?;
                self.event(output, "terminated", json!({}))
            }
            other => {
                self.stopped = true;
                self.event(
                    output,
                    "stopped",
                    json!({
                        "reason": dap_reason(other),
                        "threadId": 1,
                        "allThreadsStopped": true,
                    }),
                )
            }
        }
    }
}

/// A stop, in DAP's vocabulary.
fn dap_reason(reason: &StopReason) -> &'static str {
    match reason {
        StopReason::Breakpoint => "breakpoint",
        StopReason::Step | StopReason::Back => "step",
        StopReason::Entry => "entry",
        // `Runaway` is reported as a pause with a description, because it is not a category DAP
        // has: the adapter ran a long way and stopped to say so.
        StopReason::Pause | StopReason::Runaway => "pause",
        // The two that terminate are handled before this is reached.
        StopReason::Terminated | StopReason::Fault(_) => "exception",
    }
}
