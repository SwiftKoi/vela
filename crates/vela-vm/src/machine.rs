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

use vela_bytecode::{Module, Op, Operand};
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
        if self.status == Status::Finished {
            return Step::Halt;
        }

        loop {
            match self.execute(world) {
                Ok(Some(command)) => return Step::Yield(Box::new(command)),
                Ok(None) => {}
                Err(fault) => {
                    self.status = Status::Finished;
                    return Step::Fault(Box::new(fault));
                }
            }

            if self.status == Status::Finished {
                return Step::Halt;
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

    /// Executes one instruction, returning a command if it suspended.
    fn execute(&mut self, world: &mut World) -> Result<Option<Command>, Fault> {
        let Some(frame) = self.frames.last() else {
            self.status = Status::Finished;
            return Ok(None);
        };
        let (reference, ip) = (frame.body, frame.ip);

        let Some(body) = self.body(reference) else {
            return Err(Fault::BadFunction(ip as u32));
        };
        let Some(instr) = body.code.get(ip) else {
            // Running off the end ends a label and faults in a function: something called
            // the function and is waiting for a value it will never get.
            return match reference {
                BodyRef::Label(_) => {
                    self.status = Status::Finished;
                    Ok(None)
                }
                BodyRef::Function(index) => Err(Fault::BadFunction(index)),
            };
        };

        let (op, operand) = (instr.op, instr.operand.clone());
        // Advanced before applying, so a jump can overwrite it.
        if let Some(frame) = self.frames.last_mut() {
            frame.ip += 1;
        }

        self.apply(op, &operand, world)
    }

    /// Applies one instruction.
    fn apply(
        &mut self,
        op: Op,
        operand: &Operand,
        world: &mut World,
    ) -> Result<Option<Command>, Fault> {
        match op {
            Op::Jump => {
                self.jump(operand.u32().unwrap_or(0))?;
                Ok(None)
            }
            Op::JumpIfFalse => {
                let condition = self.pop_bool(op)?;
                if !condition {
                    self.jump(operand.u32().unwrap_or(0))?;
                }
                Ok(None)
            }
            Op::JumpIfTrue => {
                let condition = self.pop_bool(op)?;
                if condition {
                    self.jump(operand.u32().unwrap_or(0))?;
                }
                Ok(None)
            }
            Op::Return => {
                self.leave(op, operand.count())?;
                Ok(None)
            }
            Op::CallFn => {
                self.enter(BodyRef::Function(operand.u32().unwrap_or(0)))?;
                Ok(None)
            }
            Op::CallLabel => {
                self.enter(BodyRef::Label(operand.u32().unwrap_or(0)))?;
                Ok(None)
            }
            Op::CallValue => {
                let target = self.pop()?;
                let Value::Function(name) = target else {
                    return Err(Fault::WrongType {
                        op: op.name(),
                        expected: "a function",
                        found: target.type_name(),
                    });
                };
                let index = self
                    .module
                    .fns
                    .iter()
                    .position(|body| self.module.strings.get(body.name) == Some(name.as_str()))
                    .ok_or(Fault::NoLabel(name))?;
                self.enter(BodyRef::Function(u32::try_from(index).unwrap_or(u32::MAX)))?;
                Ok(None)
            }
            Op::Dispatch => {
                let tag = self.pop_int(op)?;
                self.dispatch(operand, tag)?;
                Ok(None)
            }
            Op::Yield => {
                let command = self
                    .pending
                    .take()
                    .ok_or_else(|| Fault::BadSchema("yield".to_string()))?;
                Ok(Some(command))
            }
            _ => {
                crate::exec::simple(self, op, operand, world)?;
                Ok(None)
            }
        }
    }

    /// Moves to a byte offset.
    pub(crate) fn jump(&mut self, offset: u32) -> Result<(), Fault> {
        let Some(frame) = self.frames.last_mut() else {
            return Err(Fault::Finished);
        };
        let body = match frame.body {
            BodyRef::Function(index) => self.module.fns.get(index as usize),
            BodyRef::Label(index) => self.module.labels.get(index as usize),
        }
        .ok_or(Fault::BadFunction(0))?;
        frame.ip = body
            .at_offset(offset)
            .map(|(index, _)| index)
            .ok_or(Fault::BadTarget(offset))?;
        Ok(())
    }

    /// Enters a function or label.
    ///
    /// The caller has already pushed one value per parameter, in order, so this frame's slot
    /// zero is the **first of them** rather than the top of the stack. `base` used to be
    /// `stack.len()`, which put the parameters *below* the frame: every read of a parameter then
    /// found nothing and handed back `none`, so `n == 1` was false for every `n` and a function
    /// quietly took the wrong branch instead of failing.
    pub(crate) fn enter(&mut self, body: BodyRef) -> Result<(), Fault> {
        let (exists, params) = match body {
            BodyRef::Function(index) => (
                self.module.fns.get(index as usize).is_some(),
                self.module
                    .fns
                    .get(index as usize)
                    .map_or(0, |function| function.params.len()),
            ),
            BodyRef::Label(index) => (
                self.module.labels.get(index as usize).is_some(),
                self.module
                    .labels
                    .get(index as usize)
                    .map_or(0, |label| label.params.len()),
            ),
        };
        if !exists {
            return Err(match body {
                BodyRef::Function(index) => Fault::BadFunction(index),
                BodyRef::Label(index) => Fault::BadLabel(index),
            });
        }

        let base = self.stack.len().saturating_sub(params);
        self.frames.push(Frame { body, ip: 0, base });
        Ok(())
    }

    /// Leaves the current frame, with its results on top of the stack.
    fn leave(&mut self, op: Op, arity: usize) -> Result<(), Fault> {
        if self.stack.len() < arity {
            return Err(Fault::EmptyStack { op: op.name() });
        }
        let results: Vec<Value> = self.stack.split_off(self.stack.len() - arity);

        let Some(frame) = self.frames.pop() else {
            self.status = Status::Finished;
            return Ok(());
        };

        if self.frames.is_empty() {
            // A label returning to nothing is the story ending. A function returning to
            // nothing is a bug, because something called it and is waiting.
            return match frame.body {
                BodyRef::Label(_) => {
                    self.status = Status::Finished;
                    Ok(())
                }
                BodyRef::Function(index) => Err(Fault::BadFunction(index)),
            };
        }

        self.stack.truncate(frame.base);
        self.stack.extend(results);
        Ok(())
    }

    /// Jumps through a dispatch table.
    fn dispatch(&mut self, operand: &Operand, tag: i64) -> Result<(), Fault> {
        let Operand::Tables(_, targets) = operand else {
            return Err(Fault::BadTarget(0));
        };
        let index = usize::try_from(tag).unwrap_or(usize::MAX);
        let offset = targets.get(index).copied().ok_or(Fault::BadTarget(0))?;
        self.jump(offset)
    }
}
