//! The rules for operators.
//!
//! In their own file because they are a table rather than a traversal: every operator has a rule, most
//! of them are one line, and the two that are not — arithmetic and the fallback — are where the
//! language's deliberate refusals live (`LANGUAGE.md §5.7`: no implicit numbers, no implicit
//! rendering). Keeping them together is what makes "what does `+` mean" answerable by reading one
//! screen instead of searching for the arm.

use vela_span::Span;
use vela_syntax::{BinOp, Expr, UnOp};

use crate::check::run::Checker;
use crate::error;
use crate::ty::Ty;

impl Checker<'_> {
    /// The type of a binary operation.
    pub(super) fn binary(&mut self, op: BinOp, left: &Ty, right: &Ty, span: Span) -> Ty {
        use BinOp::{
            Add, And, Coalesce, Div, Eq, Ge, Gt, In, Is, IsNot, Le, Lt, Mul, Ne, NotIn, Or, Rem,
            Sub,
        };

        match op {
            Add | Sub | Mul | Div | Rem => {
                if left.is_optional() {
                    self.report(error::needs_unwrap(left, span));
                    return Ty::Unknown;
                }
                if right.is_optional() {
                    self.report(error::needs_unwrap(right, span));
                    return Ty::Unknown;
                }
                // Only numbers can be combined, and `LANGUAGE.md §5.7` forbids converting
                // between them implicitly: a score that silently becomes `3.0` produces a
                // display bug nobody can explain.
                if left.is_numeric() && right.is_numeric() && left != right {
                    self.report(error::mixed_numbers(left, right, span));
                    return Ty::Unknown;
                }
                left.clone()
            }

            Eq | Ne => {
                if !left.accepts(right) && !right.accepts(left) {
                    self.report(error::mismatch(left, right, span));
                }
                Ty::Bool
            }

            Lt | Le | Gt | Ge => {
                if left.is_numeric() && right.is_numeric() && left != right {
                    self.report(error::mixed_numbers(left, right, span));
                }
                Ty::Bool
            }

            And | Or => Ty::Bool,
            Is | IsNot | In | NotIn => Ty::Bool,

            Coalesce => match left {
                // `a ?? b` is the payload of `a` when it has one, so the result is never
                // optional — which is the whole point of writing it.
                Ty::Optional(inner) => {
                    if !inner.accepts(right) {
                        self.report(error::mismatch(inner, right, span));
                    }
                    (**inner).clone()
                }
                _ => left.clone(),
            },
        }
    }

    /// The type of a prefix operation.
    pub(super) fn unary(&mut self, op: UnOp, operand: &Expr, span: Span) -> Ty {
        let ty = self.expr(operand);
        match op {
            UnOp::Neg => {
                if ty.is_optional() {
                    self.report(error::needs_unwrap(&Ty::Int, span));
                    return Ty::Unknown;
                }
                ty
            }
            // Both negations demand a bool and produce one; which level of the grammar they sit at
            // is already in the tree's shape, so the typing rule is the same.
            UnOp::Not | UnOp::Bang => {
                if ty != Ty::Bool && ty != Ty::Unknown {
                    self.report(error::mismatch(&Ty::Bool, &ty, operand.span()));
                }
                Ty::Bool
            }
        }
    }
}
