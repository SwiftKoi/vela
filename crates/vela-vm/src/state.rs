//! A snapshot of the machine, and how it is put back.
//!
//! `RUNTIME.md §7.2`: *"frames are snapshotted by value; because there are no host pointers,
//! this is a memcpy of plain data, not a serialization round-trip."* The machine is frames,
//! an operand stack, and a status — all plain data — so this is a copy, not a translation.
//!
//! # Frames are named, not numbered
//!
//! A frame records the body it is running. Numbering that body would be smaller and would be
//! wrong: an index into the module's function table is not stable across a recompile, and a
//! save written before an edit would resume in whatever function happened to land at that
//! index. The name is what survives, which is the same reason `World::call_stack` holds
//! labels rather than ids.
//!
//! # And a suspension is anchored, not indexed
//!
//! The same argument applies *inside* a body, where the frame's instruction pointer lives. An
//! index is a position in one particular compilation: the compiler may emit a different
//! number of instructions for the same story — a different optimization level is enough — and
//! an author's edit moves everything after it. So a suspended frame is written down with the
//! **source range of the statement it is waiting at** ([`Resume`]), and the index is carried
//! beside it as a hint.
//!
//! On restore the statement is looked up where it now is. If it is not in the body any more —
//! the story changed under the save — the restore fails ([`Fault::StaleFrame`]) rather than
//! resuming at whichever instruction inherited the index, which would run a different story
//! from the same save without saying so.

use serde::{Deserialize, Serialize};
use vela_bytecode::{FuncDef, Module, Op};
use vela_world::{Command, Value};

use crate::fault::Fault;
use crate::machine::{BodyRef, Frame, Status, Vm};

/// The source range of the statement a frame is suspended at.
///
/// This is what survives a recompile. The instruction index does not: the compiler is free to
/// emit a different number of instructions for the same source, and the faster build of the
/// same story is exactly that. The *statement* is the same either way.
///
/// The file is deliberately not part of it. A body comes from one file, so the byte range
/// alone picks out the statement within it, and a [`FileId`](vela_span::FileId) is assigned
/// by whichever compilation parsed the project — adding a file would renumber the ones after
/// it and break every save for no added precision.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Resume {
    /// Byte offset of the statement's first byte.
    pub start: u32,
    /// Byte offset one past its last byte.
    pub end: u32,
}

/// One activation, as it is written down.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct FrameState {
    /// The body, as `label:name` or `fn:name`.
    pub body: String,
    /// The instruction being executed, by index.
    ///
    /// A hint, not an anchor: see [`Resume`]. It is what a save written before
    /// [`Resume`] existed has instead of a statement, and what the loader tries first
    /// because a body that did not move costs one comparison.
    pub ip: u32,
    /// Where this frame's stack began.
    pub base: usize,
    /// The statement the frame is suspended at, if it is suspended at one.
    ///
    /// Absent for a frame that is not suspended — a caller, whose index is a return address —
    /// and for a save written before this field existed.
    #[serde(default)]
    pub resume: Option<Resume>,
}

/// The machine's whole state, at a suspension or anywhere else.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct VmState {
    /// The call stack, outermost first.
    pub frames: Vec<FrameState>,
    /// The operand stack, which holds every frame's locals.
    pub stack: Vec<Value>,
    /// The command a `Cmd` built and the `Yield` has not yet handed over.
    pub pending: Option<Command>,
    /// Whether the story has ended.
    pub finished: bool,
}

impl Vm {
    /// A copy of the machine's state.
    #[must_use]
    pub fn state(&self) -> VmState {
        VmState {
            frames: self
                .frames
                .iter()
                .map(|frame| FrameState {
                    body: body_name(&self.module, frame.body),
                    ip: u32::try_from(frame.ip).unwrap_or(u32::MAX),
                    base: frame.base,
                    resume: self.suspension(frame),
                })
                .collect(),
            stack: self.stack.clone(),
            pending: self.pending.clone(),
            finished: self.status == Status::Finished,
        }
    }

    /// The statement a frame is suspended at, if it is suspended at one.
    ///
    /// A `Yield` hands its command over and leaves the index on the instruction *after* it,
    /// so the suspension is the instruction before. A frame at index zero has not suspended —
    /// the snapshot taken before a story's first command is exactly that — and a frame whose
    /// predecessor is not a `Yield` is a caller waiting on the frame above it, which resumes
    /// from where it left off rather than from a suspension.
    fn suspension(&self, frame: &Frame) -> Option<Resume> {
        let at = frame.ip.checked_sub(1)?;
        let body = self.body(frame.body)?;
        if body.code.get(at)?.op != Op::Yield {
            return None;
        }
        let span = body.spans.get(at)?;
        Some(Resume {
            start: span.start(),
            end: span.end(),
        })
    }

    /// Puts the machine back into a state.
    ///
    /// # Errors
    ///
    /// Fails if a frame names a body this module does not have, or if a frame's suspension is
    /// not in the body it names — a save loaded against a story that has changed since it was
    /// written (`RUNTIME.md §5`).
    pub fn restore_state(&mut self, state: &VmState) -> Result<(), Fault> {
        let mut frames = Vec::with_capacity(state.frames.len());
        for frame in &state.frames {
            let (body, func) = lookup(&self.module, &frame.body)
                .ok_or_else(|| Fault::NoLabel(frame.body.clone()))?;
            let ip = resume_index(func, frame)?;
            frames.push(Frame {
                body,
                ip,
                base: frame.base,
            });
        }

        self.frames = frames;
        self.stack = state.stack.clone();
        self.pending = state.pending.clone();
        self.status = if state.finished {
            Status::Finished
        } else {
            Status::Running
        };
        Ok(())
    }
}

/// Where a written-down frame resumes, in *this* module.
///
/// With an anchor this is a lookup, and the recorded index is only a hint — which is what
/// makes a save survive a recompilation. Without one (a save written before the anchor
/// existed) the index is all there is, so it is taken as written; a body that no longer even
/// reaches that far is refused rather than run off the end of.
fn resume_index(func: &FuncDef, frame: &FrameState) -> Result<usize, Fault> {
    let recorded = frame.ip as usize;
    let Some(anchor) = frame.resume else {
        return if recorded <= func.code.len() {
            Ok(recorded)
        } else {
            Err(stale(&frame.body))
        };
    };

    // A body that did not move costs one comparison, which is the common case.
    if recorded >= 1 && is_suspension(func, recorded - 1, anchor) {
        return Ok(recorded);
    }

    let mut found = None;
    for index in 0..func.code.len() {
        if is_suspension(func, index, anchor) {
            if found.is_some() {
                // Two suspensions carry the same statement, so neither is *the* one to
                // resume at. Guessing would be the failure this whole file exists to avoid.
                return Err(stale(&frame.body));
            }
            found = Some(index);
        }
    }
    found
        .map(|index| index + 1)
        .ok_or_else(|| stale(&frame.body))
}

/// Whether the instruction at `index` is the suspension `anchor` names.
fn is_suspension(func: &FuncDef, index: usize, anchor: Resume) -> bool {
    let Some(instr) = func.code.get(index) else {
        return false;
    };
    if instr.op != Op::Yield {
        return false;
    }
    func.spans
        .get(index)
        .is_some_and(|span| span.start() == anchor.start && span.end() == anchor.end)
}

/// The failure a save whose story changed gets, rather than a wrong resume.
fn stale(body: &str) -> Fault {
    Fault::StaleFrame {
        body: body.to_string(),
    }
}

/// A body as `kind:name`, the form that survives a recompile.
fn body_name(module: &Module, body: BodyRef) -> String {
    match body {
        BodyRef::Label(index) => {
            let name = module
                .labels
                .get(index as usize)
                .and_then(|label| module.strings.get(label.name))
                .unwrap_or_default();
            format!("label:{name}")
        }
        BodyRef::Function(index) => {
            let name = module
                .fns
                .get(index as usize)
                .and_then(|function| module.strings.get(function.name))
                .unwrap_or_default();
            format!("fn:{name}")
        }
    }
}

/// The body a written-down name refers to, and the body itself.
///
/// One lookup rather than two, so the name and the body cannot disagree about what was found.
fn lookup<'a>(module: &'a Module, name: &str) -> Option<(BodyRef, &'a FuncDef)> {
    let reference = resolve(module, name)?;
    let func = match reference {
        BodyRef::Label(index) => module.labels.get(index as usize),
        BodyRef::Function(index) => module.fns.get(index as usize),
    }?;
    Some((reference, func))
}

/// The body a written-down name refers to.
fn resolve(module: &Module, name: &str) -> Option<BodyRef> {
    let (kind, rest) = name.split_once(':')?;
    match kind {
        "label" => module
            .labels
            .iter()
            .position(|label| module.strings.get(label.name) == Some(rest))
            .and_then(|index| u32::try_from(index).ok())
            .map(BodyRef::Label),
        "fn" => module
            .fns
            .iter()
            .position(|function| module.strings.get(function.name) == Some(rest))
            .and_then(|index| u32::try_from(index).ok())
            .map(BodyRef::Function),
        _ => None,
    }
}
