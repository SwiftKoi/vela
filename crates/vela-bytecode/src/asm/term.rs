//! Emitting terminators.
//!
//! Where the encoding gets interesting. Two of these are not what they look like: a `jump`
//! is a tail call in two instructions, and a `dispatch` carries its table with it.

use vela_mir::Terminator;

use crate::op::{NO_ENUM, Op, Operand};

use super::body::Emitter;
use super::body::Fixup;

impl Emitter<'_> {
    /// Emits a terminator.
    pub(super) fn terminator(&mut self, term: &Terminator) {
        match term {
            Terminator::Goto(target) => self.jump_to(Op::Jump, target.0),
            Terminator::Branch { cond, then_, else_ } => {
                let cond = *cond;
                self.value(cond);
                self.jump_to(Op::JumpIfFalse, else_.0);
                self.jump_to(Op::Jump, then_.0);
            }
            Terminator::Return(None) => self.emit(Op::Return, Operand::U32(0)),
            Terminator::Return(Some(value)) => {
                let value = *value;
                self.value(value);
                self.emit(Op::Return, Operand::U32(1));
            }
            Terminator::JumpLabel(target) => {
                // A tail call, in two instructions. `CallLabel` pushes the target's frame
                // with a return address of the instruction after it; `Return 0` then pops
                // *this* frame, so the target returns to whoever called us rather than to a
                // label that has already finished.
                self.call_label(target);
                self.emit(Op::Return, Operand::U32(0));
            }
            Terminator::CallLabel { target, ret } => {
                self.call_label(target);
                self.jump_to(Op::Jump, ret.0);
            }
            Terminator::Dispatch {
                enum_name,
                value,
                arms,
                else_,
            } => self.dispatch(enum_name, *value, arms, else_.0),
            Terminator::Yield(site) => {
                let site = *site;
                self.emit(Op::Yield, Operand::None);
                match site.result {
                    Some(result) => self.emit(Op::StoreLocal, Operand::U32(result.0)),
                    // A suspension always hands back an answer, and a command that wants
                    // none leaves it on the stack. Discarding it is not tidiness: rule 1
                    // requires every path into a block to arrive at the same depth, and
                    // without this the arms of a menu reach the join one deeper than the
                    // dispatch's catch-all does.
                    None => self.emit(Op::Pop, Operand::None),
                }

                // `Yield` continues at the *next instruction*, and the resume block is not
                // necessarily physically next — a menu's blocks are opened before its arms,
                // so their ids do not follow the order they were opened in. Without this
                // jump, a suspension falls through into whatever was emitted next, which
                // verifies as a stack-depth mismatch rather than as a wrong jump.
                self.jump_to(Op::Jump, site.resume.0);
            }
            // Unreachable blocks are not emitted, so a terminator that produces nothing is
            // only reached if one was, which cannot happen.
            Terminator::Unreachable => {}
        }
    }

    /// Emits a dispatch and its table.
    ///
    /// The tag is turned into a variant *position* first, because the table is indexed
    /// rather than searched — which needs the enum, and is why `EnumTag` carries one. A
    /// menu's table is over choice positions already, so it is not tagged at all.
    fn dispatch(
        &mut self,
        enum_name: &str,
        value: vela_mir::Value,
        arms: &[(vela_mir::VariantId, vela_mir::BlockId)],
        else_: u32,
    ) {
        let enum_id = if enum_name.is_empty() {
            NO_ENUM
        } else {
            self.builder
                .enum_ids
                .get(enum_name)
                .copied()
                .unwrap_or(NO_ENUM)
        };

        self.value(value);
        if enum_id != NO_ENUM {
            self.emit(Op::EnumTag, Operand::U32(enum_id));
        }

        let at = self.code.len();
        self.emit(
            Op::Dispatch,
            Operand::Tables(enum_id, vec![0; arms.len() + 1]),
        );
        for (entry, (_, block)) in arms.iter().enumerate() {
            self.fixups.push(Fixup::Table {
                at,
                entry,
                block: block.0 as usize,
            });
        }
        self.fixups.push(Fixup::Table {
            at,
            entry: arms.len(),
            block: else_ as usize,
        });
    }

    /// Emits a label call.
    pub(super) fn call_label(&mut self, target: &vela_mir::LabelRef) {
        let index = self
            .builder
            .labels
            .get(&target.label)
            .copied()
            .unwrap_or(u32::MAX);
        self.emit(Op::CallLabel, Operand::U32(index));
    }
}
