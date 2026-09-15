//! Lowering the expressions that decide control flow.
//!
//! Short-circuiting and `??` are not arithmetic: both have to *branch*, and both have to
//! leave a value behind at the join. Keeping them apart from the straight-line operators
//! makes that difference visible in the file layout, not just in a comment.

use vela_span::Span;
use vela_syntax::{BinOp, Expr};
use vela_types::Ty;

use crate::ir::{Operand, Place, StmtKind, Terminator, Value};
use crate::lower::Lowerer;

impl Lowerer<'_> {
    /// A binary operation that is not short-circuiting.
    pub(crate) fn binary(
        &mut self,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
        ty: Ty,
        span: Span,
    ) -> Value {
        match op {
            BinOp::And | BinOp::Or => return self.short_circuit(op, lhs, rhs, span),
            BinOp::Coalesce => return self.coalesce(lhs, rhs, ty, span),
            _ => {}
        }

        let a = self.expr(lhs);
        let b = self.expr(rhs);
        let dst = self.temp(ty);
        self.emit(
            StmtKind::Assign {
                dst: Place::Local(dst),
                op,
                a,
                b,
            },
            span,
        );
        Value::Slot(dst)
    }

    /// `a and b` / `a or b`, which must not evaluate `b` when `a` decides it.
    pub(crate) fn short_circuit(&mut self, op: BinOp, lhs: &Expr, rhs: &Expr, span: Span) -> Value {
        let dst = self.temp(Ty::Bool);
        let a = self.expr(lhs);
        self.emit(
            StmtKind::Load {
                dst: Place::Local(dst),
                src: Operand::Value(a),
            },
            span,
        );

        let head = self.body.current();
        let right = self.body.open();
        let join = self.body.open();
        self.body.start(head);

        // `and` returns early on false, `or` on true — the same branch with the arms
        // swapped, which is why they share a lowering.
        let (then_, else_) = match op {
            BinOp::Or => (join, right),
            _ => (right, join),
        };
        self.body.seal(Terminator::Branch {
            cond: Value::Slot(dst),
            then_,
            else_,
        });

        self.body.start(right);
        let b = self.expr(rhs);
        self.emit(
            StmtKind::Load {
                dst: Place::Local(dst),
                src: Operand::Value(b),
            },
            span,
        );
        self.body.seal(Terminator::Goto(join));

        self.body.start(join);
        Value::Slot(dst)
    }

    /// `a ?? b`, which is `a`'s payload when it has one and `b` otherwise.
    pub(crate) fn coalesce(&mut self, lhs: &Expr, rhs: &Expr, ty: Ty, span: Span) -> Value {
        let dst = self.temp(ty);
        let base = self.expr(lhs);
        let missing = self.temp(Ty::Bool);
        self.emit(
            StmtKind::IsNone {
                dst: Place::Local(missing),
                base,
            },
            span,
        );

        let head = self.body.current();
        let some = self.body.open();
        let none = self.body.open();
        let join = self.body.open();
        self.body.start(head);
        self.body.seal(Terminator::Branch {
            cond: Value::Slot(missing),
            then_: none,
            else_: some,
        });

        self.body.start(some);
        self.emit(
            StmtKind::Unwrap {
                dst: Place::Local(dst),
                base,
            },
            span,
        );
        self.body.seal(Terminator::Goto(join));

        self.body.start(none);
        let fallback = self.expr(rhs);
        self.emit(
            StmtKind::Load {
                dst: Place::Local(dst),
                src: Operand::Value(fallback),
            },
            span,
        );
        self.body.seal(Terminator::Goto(join));

        self.body.start(join);
        Value::Slot(dst)
    }

    /// `a if c else b`.
    pub(crate) fn conditional(
        &mut self,
        cond: &Expr,
        then_: &Expr,
        else_: &Expr,
        ty: Ty,
        span: Span,
    ) -> Value {
        let dst = self.temp(ty);
        let condition = self.expr(cond);

        let head = self.body.current();
        let taken = self.body.open();
        let other = self.body.open();
        let join = self.body.open();
        self.body.start(head);
        self.body.seal(Terminator::Branch {
            cond: condition,
            then_: taken,
            else_: other,
        });

        for (block, branch) in [(taken, then_), (other, else_)] {
            self.body.start(block);
            let value = self.expr(branch);
            self.emit(
                StmtKind::Load {
                    dst: Place::Local(dst),
                    src: Operand::Value(value),
                },
                span,
            );
            self.body.seal(Terminator::Goto(join));
        }

        self.body.start(join);
        Value::Slot(dst)
    }
}
