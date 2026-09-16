//! Applying one instruction, and the control flow that moves between them.
//!
//! `exec` owns the data instructions — what a *value* is — and this owns the rest: the jumps,
//! calls, returns, and table dispatch that decide *where control goes*, plus the `Yield` that
//! hands a command to the host. [`Vm::execute`] is the single entry: it reads the frame's
//! instruction, advances the pointer past it, and sends the opcode on.
//!
//! The pointer is advanced *before* the instruction is applied, so a jump simply overwrites it —
//! one rule instead of an increment in every branch that does not move.

use vela_bytecode::{Op, Operand};
use vela_world::{Command, Value, World};

use crate::fault::Fault;
use crate::machine::{BodyRef, Frame, Status, Vm};

impl Vm {
    /// Executes one instruction, returning a command if it suspended.
    pub(crate) fn execute(&mut self, world: &mut World) -> Result<Option<Command>, Fault> {
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
        if let BodyRef::Label(index) = body {
            self.record_label(index);
        }
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
