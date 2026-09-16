//! The machine: frames, an operand stack, and the step loop.
//!
//! Single-threaded and synchronous, with exactly four outcomes (`RUNTIME.md §1.2`). The VM
//! never calls a renderer: it builds a `Command`, suspends, and waits to be resumed. That
//! boundary is what makes headless testing, replay, and save/restore the same mechanism
//! rather than three subsystems.
//!
//! # Locals live on the stack
//!
//! A frame records where the stack began, and a slot is addressed from there. A `Return`
//! truncates back to that point and pushes its results, so a call's locals never collide
//! with its caller's — which is the one invariant this file has to maintain carefully, and
//! the one the verifier's rule 1 exists to make checkable.

use vela_bytecode::Module;
use vela_world::{Command, Value, World};

use crate::fault::Fault;

/// What one call to [`Vm::run`] ended with.
#[derive(Clone, PartialEq, Debug)]
pub enum Step {
    /// More work is available.
    Continue,
    /// Suspend: present this, then resume with an [`crate::Input`].
    Yield(Box<Command>),
    /// The story ended.
    Halt,
    /// A compiler bug.
    Fault(Box<Fault>),
}

/// Which table a frame's body lives in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum BodyRef {
    /// A function, by index into `Module::fns`.
    Function(u32),
    /// A label, by index into `Module::labels`.
    Label(u32),
}

/// One activation.
#[derive(Clone, Debug)]
pub(crate) struct Frame {
    pub(crate) body: BodyRef,
    /// The instruction being executed, by index.
    pub(crate) ip: usize,
    /// Where this frame's stack began, which is also its slot zero.
    pub(crate) base: usize,
}

/// Whether the story is running or over.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Status {
    Ready,
    Running,
    Finished,
}

/// The interpreter.
pub struct Vm {
    pub(crate) module: Module,
    pub(crate) frames: Vec<Frame>,
    pub(crate) stack: Vec<Value>,
    /// The command `Cmd` built, waiting for the `Yield` that hands it over.
    pub(crate) pending: Option<Command>,
    pub(crate) status: Status,
    /// The labels entered so far, by index, in the order they were first entered.
    ///
    /// Observational state rather than story state. `vela test`'s `cover labels` is a question about a
    /// *run* — which labels did this playthrough go through — and the machine is the only thing that
    /// knows. It is deliberately not part of a save: a save records where the story is, and a load
    /// resuming with an empty list at worst makes a coverage report for a resumed run, which is a
    /// report nobody asks for.
    pub(crate) entered: Vec<u32>,
}

impl Vm {
    /// A VM over a module.
    #[must_use]
    pub fn new(module: Module) -> Self {
        Self {
            module,
            frames: Vec::new(),
            stack: Vec::new(),
            pending: None,
            status: Status::Ready,
            entered: Vec::new(),
        }
    }

    /// The module it is running.
    #[must_use]
    pub fn module(&self) -> &Module {
        &self.module
    }

    /// Whether the story has finished.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.status == Status::Finished
    }

    /// How deep the call stack is.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.frames.len()
    }

    /// Begins at a label.
    ///
    /// # Errors
    ///
    /// Fails if the label is not in the module, which means the entry point named in a
    /// manifest does not match the code that was compiled.
    pub fn start(&mut self, label: &str) -> Result<(), Fault> {
        let index = self
            .module
            .labels
            .iter()
            .position(|candidate| self.module.strings.get(candidate.name) == Some(label))
            .ok_or_else(|| Fault::NoLabel(label.to_string()))?;

        let index = u32::try_from(index).unwrap_or(u32::MAX);
        self.record_label(index);
        self.frames.push(Frame {
            body: BodyRef::Label(index),
            ip: 0,
            base: 0,
        });
        self.status = Status::Running;
        Ok(())
    }

    /// Runs until it must suspend, or until the story ends.
    pub fn run(&mut self, world: &mut World) -> Step {
        loop {
            match self.step(world) {
                Step::Continue => {}
                outcome => return outcome,
            }
        }
    }

    /// Executes exactly one instruction.
    ///
    /// This is the primitive a debugger is built on (`RUNTIME.md §9`): [`Vm::run`] is this in a
    /// loop, and a caller that wants to stop *between* instructions — on a label, on a line, or
    /// for one step — drives this and asks [`Vm::site`] where it is before each call. Pull-based
    /// rather than a hook the machine calls out to, so a build that never steps pays nothing.
    pub fn step(&mut self, world: &mut World) -> Step {
        if self.status == Status::Finished {
            return Step::Halt;
        }

        match self.execute(world) {
            Ok(Some(command)) => Step::Yield(Box::new(command)),
            Ok(None) if self.status == Status::Finished => Step::Halt,
            Ok(None) => Step::Continue,
            Err(fault) => {
                self.status = Status::Finished;
                Step::Fault(Box::new(fault))
            }
        }
    }

    /// Resumes after a suspension, with the host's answer.
    ///
    /// The answer is pushed where the command was, so a `Yield` with a result slot leaves
    /// it for the following `StoreLocal`. A command that wants no answer is simply not
    /// resumed — which is what makes "advance the dialogue" and "pick a choice" the same
    /// mechanism with different shapes.
    pub fn resume(&mut self, world: &mut World, answer: Value) -> Step {
        self.stack.push(answer);
        self.run(world)
    }

    /// Resumes after a suspension and executes exactly one instruction.
    ///
    /// The debugger's counterpart to [`Vm::resume`], for the same reason [`Vm::step`] exists:
    /// an answer has to be delivered, but the machine must then stop again rather than run on
    /// to the next suspension.
    pub fn resume_step(&mut self, world: &mut World, answer: Value) -> Step {
        self.stack.push(answer);
        self.step(world)
    }
}
