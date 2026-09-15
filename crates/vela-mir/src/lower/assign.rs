//! Lowering assignment.
//!
//! An assignment is the one place a *place* and a *value* meet: the left side names a
//! location, the right side computes something to put there. Compound forms make that
//! explicit — `x += 1` reads the place and writes it, in that order.

use vela_span::Span;
use vela_syntax::{AssignOp, AssignStmt, BinOp};
use vela_types::Ty;

use crate::ir::{Operand, Place, StmtKind, Value};
use crate::lower::Lowerer;

impl Lowerer<'_> {
    /// `place = value`, and the compound forms.
    pub(crate) fn assign(&mut self, stmt: &AssignStmt) {
        let Some(dst) = self.place(&stmt.target) else {
            // The checker has already rejected this target. Lowering must not invent a
            // location and write to it.
            return;
        };

        if stmt.op == AssignOp::Assign {
            self.expr_into(dst, &stmt.value);
            return;
        }

        // A compound assignment reads the place as well as writing it, so the read has to
        // happen before the value is evaluated — which is also the order left-to-right
        // evaluation implies.
        let current = self.read_place(dst.clone(), stmt.span);
        let value = self.expr(&stmt.value);
        let op = match stmt.op {
            AssignOp::Add => BinOp::Add,
            AssignOp::Sub => BinOp::Sub,
            AssignOp::Mul => BinOp::Mul,
            AssignOp::Div => BinOp::Div,
            AssignOp::Assign => unreachable!("handled above"),
        };
        self.emit(
            StmtKind::Assign {
                dst,
                op,
                a: current,
                b: value,
            },
            stmt.span,
        );
    }

    /// Reads the value a place currently holds.
    pub(crate) fn read_place(&mut self, place: Place, span: Span) -> Value {
        match place {
            Place::Local(slot) => Value::Slot(slot),
            Place::Default(id) => {
                let dst = self.temp(Ty::Unknown);
                self.emit(
                    StmtKind::Load {
                        dst: Place::Local(dst),
                        src: Operand::Default(id),
                    },
                    span,
                );
                Value::Slot(dst)
            }
            Place::Field { base, field } => {
                let base = self.read_place(*base, span);
                let dst = self.temp(Ty::Unknown);
                self.emit(
                    StmtKind::Load {
                        dst: Place::Local(dst),
                        src: Operand::Field { base, field },
                    },
                    span,
                );
                Value::Slot(dst)
            }
            Place::Index { base, index } => {
                let base = self.read_place(*base, span);
                let dst = self.temp(Ty::Unknown);
                self.emit(
                    StmtKind::Load {
                        dst: Place::Local(dst),
                        src: Operand::Index { base, index },
                    },
                    span,
                );
                Value::Slot(dst)
            }
        }
    }
}
