//! What a debugger asks the machine: where it is, and what the frames hold.
//!
//! The tracing interface is **pull-based** (`RUNTIME.md §9`). The machine executes one
//! instruction at a time ([`Vm::step`](crate::Vm::step)) and answers questions about its
//! position ([`Vm::site`]) and its frames ([`Vm::call_stack`], [`Vm::locals`]); a debugger
//! pauses by *not* stepping any further. There is no hook the machine calls out to, so a
//! build that never steps and never asks pays nothing for the interface existing — which is
//! the "no runtime cost in shipped games" half of §9, made true by construction rather than by
//! a feature flag.
//!
//! # Why a release module cannot be stepped through by source
//!
//! A breakpoint on a line is a breakpoint on a [`Span`], and spans are exactly what a build
//! without debug info drops (`Header::FLAG_DEBUG`, `BYTECODE.md §5`). [`Vm::site`] still names
//! the body and the instruction; it simply has no span to report, so `span` is `None` and a
//! line breakpoint has nothing to match. That is what "trace hooks are compiled out of release
//! builds" means here: the machine is the same machine, but a module built `--release` carries
//! no source map to break on, and a debugger that needs one refuses the module rather than
//! guessing.

use vela_bytecode::FuncDef;
use vela_span::Span;
use vela_world::Value;

use crate::machine::{BodyRef, Vm};

/// Where the machine is, for a debugger: the body about to run and the instruction within it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Site {
    /// The body being run — a label's or a function's name.
    pub body: String,
    /// Whether the body is a label rather than a function. A label breakpoint matches on this.
    pub label: bool,
    /// The instruction about to execute, by index in the body.
    pub ip: usize,
    /// Where that instruction came from, when the module carries debug info.
    pub span: Option<Span>,
    /// How deep the call stack is. Zero is the outermost frame, which is the one a story starts
    /// in; `step out` waits for this to get smaller.
    pub depth: usize,
}

/// One frame of the call stack, as a debugger presents it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FrameInfo {
    /// The body, by name.
    pub body: String,
    /// Whether it is a label.
    pub label: bool,
    /// The instruction it is at, by index in the body.
    pub ip: usize,
    /// Where that instruction came from, when the module has spans.
    pub span: Option<Span>,
    /// Its depth: zero for the outermost frame.
    pub depth: usize,
}

/// One local slot and its current value.
#[derive(Clone, PartialEq, Debug)]
pub struct DebugLocal {
    /// The name the author gave it, or empty for a compiler temporary.
    pub name: String,
    /// Its slot number, which is also its index in the frame's `locals`.
    pub slot: u32,
    /// What it holds right now.
    pub value: Value,
}

impl Vm {
    /// Where the machine is: the instruction about to execute.
    ///
    /// `None` when nothing is running — the story has not started, or it has ended and the last
    /// frame is gone. A truthful "nowhere" rather than an invented position.
    #[must_use]
    pub fn site(&self) -> Option<Site> {
        let frame = self.frames.last()?;
        let body = self.body(frame.body)?;
        Some(Site {
            body: self.module.strings.get(body.name)?.to_string(),
            label: matches!(frame.body, BodyRef::Label(_)),
            ip: frame.ip,
            span: self.source_span(body, frame.ip),
            depth: self.frames.len().saturating_sub(1),
        })
    }

    /// The call stack, outermost frame first.
    ///
    /// This is what `step over`/`step out` are expressed over: a step over waits for a frame at
    /// the same or a shallower depth, and a step out waits for the current depth to go away.
    #[must_use]
    pub fn call_stack(&self) -> Vec<FrameInfo> {
        self.frames
            .iter()
            .enumerate()
            .map(|(depth, frame)| {
                let body = self.body(frame.body);
                FrameInfo {
                    body: body
                        .and_then(|body| self.module.strings.get(body.name))
                        .unwrap_or_default()
                        .to_string(),
                    label: matches!(frame.body, BodyRef::Label(_)),
                    ip: frame.ip,
                    span: body.and_then(|body| self.source_span(body, frame.ip)),
                    depth,
                }
            })
            .collect()
    }

    /// The top frame's slots that hold a value, with the names the module carries.
    ///
    /// Names come from debug info (`BYTECODE.md §5`): a slot the compiler made for itself has an
    /// empty name and is reported as one rather than hidden, because a value the debugger cannot
    /// see is a value it cannot explain. A slot nothing has written yet holds nothing and is
    /// absent — a variable that does not exist yet is not the same as one whose value is `none`.
    #[must_use]
    pub fn locals(&self) -> Vec<DebugLocal> {
        self.locals_at(self.frames.len().saturating_sub(1))
    }

    /// One frame's slots that hold a value, by name.
    ///
    /// `frame` is the frame's position in the call stack, zero for the outermost. A caller
    /// starts from [`Vm::call_stack`] to know which frames there are; a frame that is not there
    /// has no locals, which is an empty list rather than an error.
    #[must_use]
    pub fn locals_at(&self, frame: usize) -> Vec<DebugLocal> {
        let Some(frame) = self.frames.get(frame) else {
            return Vec::new();
        };
        let Some(body) = self.body(frame.body) else {
            return Vec::new();
        };

        body.locals
            .iter()
            .enumerate()
            .filter_map(|(index, local)| {
                let value = self.stack.get(frame.base.checked_add(index)?)?.clone();
                Some(DebugLocal {
                    name: if self.module.header.has_debug() {
                        self.module
                            .strings
                            .get(local.name)
                            .unwrap_or_default()
                            .to_string()
                    } else {
                        String::new()
                    },
                    slot: u32::try_from(index).unwrap_or(u32::MAX),
                    value,
                })
            })
            .collect()
    }

    /// The source span of an instruction, when the module carries debug info.
    ///
    /// A module built `--release` sets `Header::FLAG_DEBUG` to 0 (`BYTECODE.md §5`), and from
    /// then on the machine reports no span — even though the body still holds the instruction,
    /// because a [`Span`] is the source map and a shipped build does not ship one. This is the
    /// single place the "no debug info" flag is read, so the whole debug view turns off together
    /// rather than one query at a time.
    fn source_span(&self, body: &FuncDef, ip: usize) -> Option<Span> {
        if !self.module.header.has_debug() {
            return None;
        }
        body.spans.get(ip).copied()
    }
}
