//! Looking at a stopped story: the stack, the variables, and an expression.
//!
//! Everything here is a *question about a paused machine*, which is why none of it moves the
//! story: `RUNTIME.md §9` keeps the machine pull-based, and reading is a separate act from
//! stepping. `World` is shown alongside the frame's slots because in a visual novel most of what
//! an author wants to see is world state, not locals.

use std::io::{self, Write};

use serde_json::{Value, json};
use vela_span::{FileId, Span};

use super::state::{Scope, Server};

impl Server {
    /// `threads` — a story runs in one thread, and saying so is what lets a client ask for a
    /// stack.
    pub(super) fn threads(&mut self, request_seq: i64, output: &mut dyn Write) -> io::Result<()> {
        self.respond(
            output,
            request_seq,
            "threads",
            json!({ "threads": [{ "id": 1, "name": "story" }] }),
        )
    }

    /// `stackTrace` — the call stack, top first, which is the direction DAP counts in.
    pub(super) fn stack_trace(
        &mut self,
        request_seq: i64,
        output: &mut dyn Write,
    ) -> io::Result<()> {
        let frames = self.stack_frames();
        let total = frames.len();
        self.respond(
            output,
            request_seq,
            "stackTrace",
            json!({ "stackFrames": frames, "totalFrames": total }),
        )
    }

    /// `scopes` — the two containers a frame has: its own slots, and `World`.
    pub(super) fn scopes(
        &mut self,
        arguments: &Value,
        request_seq: i64,
        output: &mut dyn Write,
    ) -> io::Result<()> {
        let frame = frame_id(arguments);
        let locals = self.allocate(Scope::Locals(frame));
        let world = self.allocate(Scope::World);
        self.respond(
            output,
            request_seq,
            "scopes",
            json!({
                "scopes": [
                    { "name": "Locals", "variablesReference": locals, "expensive": false },
                    { "name": "World", "variablesReference": world, "expensive": false },
                ],
            }),
        )
    }

    /// `variables` — the contents of a scope handed out earlier.
    pub(super) fn variables(
        &mut self,
        arguments: &Value,
        request_seq: i64,
        output: &mut dyn Write,
    ) -> io::Result<()> {
        let reference = arguments
            .get("variablesReference")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let variables = match self.scopes.get(&reference).copied() {
            Some(Scope::Locals(frame)) => self.locals_variables(frame),
            Some(Scope::World) => self.world_variables(),
            None => Vec::new(),
        };
        self.respond(
            output,
            request_seq,
            "variables",
            json!({ "variables": variables }),
        )
    }

    /// `evaluate` — a name is answered from the paused state; anything else is checked.
    ///
    /// A name is what a variable view shows and what a hover asks about, so it is answered with
    /// its *value*. Anything else is an expression this version cannot produce a value for, and
    /// the checker is asked instead (`TOOLING.md §6`): it either rejects the expression with a
    /// normal `Exxx`, which is the whole point, or types it — and the type is reported as the
    /// answer rather than a value invented for it.
    pub(super) fn evaluate(
        &mut self,
        arguments: &Value,
        request_seq: i64,
        output: &mut dyn Write,
    ) -> io::Result<()> {
        let expression = arguments
            .get("expression")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let frame = frame_id(arguments);

        if let Some((value, ty)) = self.name_value(frame, &expression) {
            return self.respond(
                output,
                request_seq,
                "evaluate",
                json!({ "result": value, "type": ty, "variablesReference": 0 }),
            );
        }

        let Some((file, name, text)) = self.paused_file() else {
            return self.fail(
                output,
                request_seq,
                "evaluate",
                "the story is not paused in a source file",
            );
        };

        match crate::evaluate::evaluate(
            file,
            &name,
            &text,
            &expression,
            &self.locals_in_scope(frame),
        ) {
            Ok(ty) => self.respond(
                output,
                request_seq,
                "evaluate",
                json!({
                    "result": format!("{ty} (checked; only names are evaluated)"),
                    "type": ty,
                    "variablesReference": 0,
                }),
            ),
            Err(errors) => self.fail(output, request_seq, "evaluate", &errors.join("; ")),
        }
    }

    /// The stack, deepest frame first, as DAP wants it.
    fn stack_frames(&self) -> Vec<Value> {
        let Some(debuggee) = &self.debuggee else {
            return Vec::new();
        };

        let mut frames: Vec<Value> = debuggee
            .timeline()
            .call_stack()
            .iter()
            .map(|frame| {
                let (path, line) = self.location(frame.span);
                json!({
                    "id": frame.depth,
                    "name": frame.body,
                    "line": line,
                    "column": 1,
                    "source": { "name": base_name(&path), "path": path },
                })
            })
            .collect();
        frames.reverse();
        frames
    }

    /// The file and *one-based* line a span points at, which is the only place the off-by-one
    /// between the compiler and the protocol is allowed to happen.
    fn location(&self, span: Option<Span>) -> (String, u32) {
        let Some(span) = span else {
            return (String::new(), 1);
        };
        let path = self.program.name_of(span.file()).unwrap_or("").to_string();
        let line = self
            .program
            .position_of(span)
            .map_or(1, |position| position.line + 1);
        (path, line)
    }

    /// A frame's named slots, with their values and types.
    fn locals_variables(&self, frame: usize) -> Vec<Value> {
        let Some(debuggee) = &self.debuggee else {
            return Vec::new();
        };
        debuggee
            .timeline()
            .locals_at(frame)
            .into_iter()
            .filter(|local| !local.name.is_empty())
            .map(|local| {
                let ty = self.local_type(frame, local.slot);
                json!({
                    "name": local.name,
                    "value": local.value.to_string(),
                    "type": ty,
                    "variablesReference": 0,
                })
            })
            .collect()
    }

    /// `World`'s `default`s — the state a visual novel's story is actually made of.
    fn world_variables(&self) -> Vec<Value> {
        let Some(debuggee) = &self.debuggee else {
            return Vec::new();
        };
        debuggee
            .timeline()
            .world()
            .iter()
            .map(|(name, value)| {
                json!({
                    "name": name,
                    "value": value.to_string(),
                    "type": self.world_type(name),
                    "variablesReference": 0,
                })
            })
            .collect()
    }

    /// The source name of a slot in a frame, from the module's own debug info.
    fn local_type(&self, frame: usize, slot: u32) -> String {
        let Some(debuggee) = &self.debuggee else {
            return String::new();
        };
        let Some(info) = debuggee.timeline().call_stack().into_iter().nth(frame) else {
            return String::new();
        };

        let module = self.program.module();
        let body = if info.label {
            module
                .labels
                .iter()
                .find(|body| module.strings.get(body.name) == Some(info.body.as_str()))
        } else {
            module
                .fns
                .iter()
                .find(|body| module.strings.get(body.name) == Some(info.body.as_str()))
        };
        body.and_then(|body| body.locals.get(slot as usize))
            .map_or_else(String::new, |local| self.program.type_source(local.ty))
    }

    /// The declared type of a `default`, by name.
    fn world_type(&self, name: &str) -> String {
        let module = self.program.module();
        module
            .defaults
            .iter()
            .find(|default| module.strings.get(default.name) == Some(name))
            .map_or_else(String::new, |default| self.program.type_source(default.ty))
    }

    /// The value a bare name has, if it is one.
    fn name_value(&self, frame: usize, expression: &str) -> Option<(String, String)> {
        let name = expression.trim();
        if !vela_syntax::is_name(name) {
            return None;
        }
        let debuggee = self.debuggee.as_ref()?;

        if let Some(local) = debuggee
            .timeline()
            .locals_at(frame)
            .into_iter()
            .find(|local| local.name == name)
        {
            return Some((local.value.to_string(), self.local_type(frame, local.slot)));
        }
        let value = debuggee.timeline().world().get(name)?;
        Some((value.to_string(), self.world_type(name)))
    }

    /// The paused file, its name, and its text — what an expression is checked against.
    fn paused_file(&self) -> Option<(FileId, String, String)> {
        let file = self.debuggee.as_ref()?.site()?.span?.file();
        let name = self.program.name_of(file)?.to_string();
        let text = self.program.text_of(file)?.to_string();
        Some((file, name, text))
    }

    /// A frame's named slots as `(name, type)`, for an expression's scope.
    fn locals_in_scope(&self, frame: usize) -> Vec<(String, String)> {
        let Some(debuggee) = &self.debuggee else {
            return Vec::new();
        };
        debuggee
            .timeline()
            .locals_at(frame)
            .into_iter()
            .filter(|local| !local.name.is_empty())
            .map(|local| {
                let ty = self.local_type(frame, local.slot);
                (local.name, ty)
            })
            .collect()
    }
}

/// The frame a request names, or the top one when it names none.
fn frame_id(arguments: &Value) -> usize {
    usize::try_from(
        arguments
            .get("frameId")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .max(0),
    )
    .unwrap_or(0)
}

/// The last segment of a path, for a client that shows it in a frame label.
fn base_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}
