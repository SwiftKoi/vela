//! The parts of the machine that read and write state.
//!
//! A child module of the interpreter so that the machine's control flow and its data
//! movement stay in separate files — and so that each can be read without the other.

use vela_world::Value as WorldValue;

use crate::ir::{BlockId, Const, ConstId, LabelRef, Module, Terminator};

use super::machine::{BodyRef, Continuation, Frame, Machine, Outcome, Step};

impl Machine<'_> {
    /// Executes a terminator.
    pub(super) fn terminator(&mut self, term: &Terminator) -> Result<Step, String> {
        match term {
            Terminator::Goto(target) => {
                self.jump_to(*target);
                Ok(Step::Continue)
            }
            Terminator::Branch { cond, then_, else_ } => {
                let taken = self.read(*cond).as_bool().unwrap_or(false);
                self.jump_to(if taken { *then_ } else { *else_ });
                Ok(Step::Continue)
            }
            Terminator::Return(value) => {
                let value = value.map_or(WorldValue::None, |value| self.read(value));
                self.leave(value);
                Ok(Step::Continue)
            }
            Terminator::JumpLabel(target) => {
                let index = self.label_index(target)?;
                let slots = self.slots_for(index);
                if let Some(frame) = self.frames.last_mut() {
                    frame.body = BodyRef::Label(index);
                    frame.block = BlockId::ENTRY;
                    frame.statement = 0;
                    frame.slots = slots;
                }
                Ok(Step::Continue)
            }
            Terminator::CallLabel { target, ret } => {
                let index = self.label_index(target)?;
                let slots = self.slots_for(index);
                self.frames.push(Frame {
                    body: BodyRef::Label(index),
                    block: BlockId::ENTRY,
                    statement: 0,
                    slots,
                    ret: Some(Continuation {
                        block: *ret,
                        statement: 0,
                        destination: None,
                    }),
                });
                Ok(Step::Entered)
            }
            Terminator::Dispatch {
                enum_name,
                value,
                arms,
                else_,
            } => {
                let value = self.read(*value);
                let target = self.dispatch_target(enum_name, &value, arms);
                self.jump_to(target.unwrap_or(*else_));
                Ok(Step::Continue)
            }
            Terminator::Yield(site) => self.suspend(site),
            Terminator::Unreachable => Err("control reached an unreachable block".to_string()),
        }
    }

    /// Which arm a dispatched value selects, if any.
    pub(super) fn dispatch_target(
        &self,
        enum_name: &str,
        value: &WorldValue,
        arms: &[(crate::ir::VariantId, BlockId)],
    ) -> Option<BlockId> {
        // A menu's table is over positions, so the tag is the integer itself.
        if enum_name.is_empty() {
            let WorldValue::Int(position) = value else {
                return None;
            };
            return arms
                .iter()
                .find(|(variant, _)| i64::from(variant.0) == *position)
                .map(|(_, block)| *block);
        }

        let variant = value.variant()?;
        let definition = self.module.enum_named(enum_name)?;
        let id = definition.index_of(variant)?;
        arms.iter()
            .find(|(candidate, _)| *candidate == id)
            .map(|(_, block)| *block)
    }

    /// Finds a label, refusing one that belongs to another module.
    pub(super) fn label_index(&self, target: &LabelRef) -> Result<usize, String> {
        if let Some(module) = &target.module
            && module != self.module.name.as_str()
        {
            return Err(format!(
                "`{}` is in `{module}`, and this interpreter links one module",
                target.label
            ));
        }
        self.module
            .labels
            .iter()
            .position(|body| body.name.as_str() == target.label)
            .ok_or_else(|| format!("no label `{}`", target.label))
    }

    /// The slots a label starts with.
    pub(super) fn slots_for(&self, index: usize) -> Vec<WorldValue> {
        let count = self
            .module
            .labels
            .get(index)
            .map_or(0, |body| body.locals.len());
        vec![WorldValue::None; count]
    }

    /// Moves to a block at its first statement.
    pub(super) fn jump_to(&mut self, block: BlockId) {
        if let Some(frame) = self.frames.last_mut() {
            frame.block = block;
            frame.statement = 0;
        }
    }

    /// Pops a frame, delivering its value or ending the story.
    pub(super) fn leave(&mut self, value: WorldValue) {
        let Some(frame) = self.frames.pop() else {
            self.finished = Some(Outcome::Halted);
            return;
        };

        let Some(continuation) = frame.ret else {
            // A top-level label returning is the story ending.
            self.finished = Some(Outcome::Halted);
            return;
        };

        if let Some(destination) = &continuation.destination {
            self.write(destination, value);
        }
        if let Some(caller) = self.frames.last_mut() {
            caller.block = continuation.block;
            caller.statement = continuation.statement;
        }
    }
}

/// The runtime value of a constant.
#[must_use]
pub(super) fn constant(module: &Module, id: ConstId) -> WorldValue {
    match module.pool.get(id) {
        Some(Const::Bool(flag)) => WorldValue::Bool(*flag),
        Some(Const::Int(number)) => WorldValue::Int(*number),
        Some(Const::Float(number)) => WorldValue::Float(*number),
        Some(Const::Str(text)) => WorldValue::Str(text.clone()),
        Some(Const::Variant { enum_name, variant }) => WorldValue::Enum {
            name: enum_name.clone(),
            variant: variant.clone(),
            fields: Vec::new(),
        },
        Some(Const::Function(name)) => WorldValue::Function(name.clone()),
        Some(Const::None) | None => WorldValue::None,
    }
}
