//! Lowering expressions.

use vela_span::Span;
use vela_syntax::{BinOp, Expr, StrPart};
use vela_types::Ty;

use crate::ir::{
    Builtin, Callee, Const, DefaultId, FuncRef, Operand, Place, Slot, StmtKind, Value,
};
use crate::lower::Lowerer;

impl Lowerer<'_> {
    /// Lowers an expression into something readable.
    ///
    /// Every result lands in a slot or a constant, which is what gives codegen a value with
    /// a known type at every use site without a second inference pass.
    pub(crate) fn expr(&mut self, expr: &Expr) -> Value {
        let ty = self.type_of(expr);
        let span = expr.span();

        match expr {
            Expr::Int { value, .. } => self.constant(Const::Int(*value)),
            Expr::Float { value, .. } => self.constant(Const::Float(*value)),
            Expr::Bool { value, .. } => self.constant(Const::Bool(*value)),
            Expr::None { .. } => self.constant(Const::None),
            // A path is an asset reference; as a value it is the path as written. Whether
            // the asset exists is a build question (`BUILD_AND_ASSETS.md §9`).
            Expr::Path { value, .. } => self.constant(Const::Str(value.clone())),
            Expr::Str { parts, .. } => self.string(parts),
            Expr::Name { name, .. } => self.read_name(name, ty, span),
            Expr::Paren { inner, .. } => self.expr(inner),
            Expr::List { .. } | Expr::Map { .. } => self.literal(expr, ty, span),
            Expr::Field { base, name, .. } => self.field(base, name, ty, span),
            Expr::Index { base, index, .. } => {
                let base = self.expr(base);
                let index = self.expr(index);
                let dst = self.temp(ty);
                self.emit(
                    StmtKind::Load {
                        dst: Place::Local(dst),
                        src: Operand::Index { base, index },
                    },
                    span,
                );
                Value::Slot(dst)
            }
            Expr::Unary { op, operand, .. } => {
                let a = self.expr(operand);
                let dst = self.temp(ty);
                self.emit(
                    StmtKind::AssignUn {
                        dst: Place::Local(dst),
                        op: *op,
                        a,
                    },
                    span,
                );
                Value::Slot(dst)
            }
            Expr::Binary { op, lhs, rhs, .. } => self.binary(*op, lhs, rhs, ty, span),
            Expr::Call { callee, args, .. } => self.call(callee, args, ty, span),
            Expr::If {
                cond, then_, else_, ..
            } => self.conditional(cond, then_, else_, ty, span),
            Expr::Lambda { params, body, .. } => {
                let index = self.lift_lambda(params, body);
                let name = self.module.fns[index as usize].name.to_string();
                self.constant(Const::Function(name))
            }
            // A span where an expression should have been. Lowering stays total and
            // produces something harmless; the parse error is what gets reported.
            Expr::Error { .. } => self.constant(Const::None),
        }
    }

    /// Lowers an expression into a destination, without a temporary where one is avoidable.
    ///
    /// `var x = a + b` lowering to two statements would be noise in every golden; going
    /// straight to the destination keeps the common cases one line, which is what makes
    /// the corpus readable enough to check by eye.
    pub(crate) fn expr_into(&mut self, dst: Place, expr: &Expr) {
        let span = expr.span();
        match expr {
            Expr::Binary { op, lhs, rhs, .. }
                if !matches!(op, BinOp::And | BinOp::Or | BinOp::Coalesce) =>
            {
                let a = self.expr(lhs);
                let b = self.expr(rhs);
                self.emit(StmtKind::Assign { dst, op: *op, a, b }, span);
            }
            Expr::Unary { op, operand, .. } => {
                let a = self.expr(operand);
                self.emit(StmtKind::AssignUn { dst, op: *op, a }, span);
            }
            Expr::Paren { inner, .. } => self.expr_into(dst, inner),
            _ => {
                let value = self.expr(expr);
                self.emit(
                    StmtKind::Load {
                        dst,
                        src: Operand::Value(value),
                    },
                    span,
                );
            }
        }
    }

    /// A list or map literal, built element by element.
    fn literal(&mut self, expr: &Expr, ty: Ty, span: Span) -> Value {
        let dst = self.temp(ty);

        match expr {
            Expr::List { items, .. } => {
                let items: Vec<Value> = items.iter().map(|item| self.expr(item)).collect();
                self.emit(
                    StmtKind::ListNew {
                        dst: Place::Local(dst),
                        items,
                    },
                    span,
                );
            }
            Expr::Map { entries, .. } => {
                let entries: Vec<(Value, Value)> = entries
                    .iter()
                    .map(|(key, value)| (self.expr(key), self.expr(value)))
                    .collect();
                self.emit(
                    StmtKind::MapNew {
                        dst: Place::Local(dst),
                        entries,
                    },
                    span,
                );
            }
            _ => return self.constant(Const::None),
        }

        Value::Slot(dst)
    }

    /// Reads a name.
    ///
    /// Four things a bare name can be, in the order they are checked: a local, a `default`,
    /// a `const`, and a function used as a value. The checker has already decided which,
    /// and each lowers to something different — a `const` is not read at run time at all,
    /// it *is* its value.
    pub(crate) fn read_name(&mut self, name: &str, ty: Ty, span: Span) -> Value {
        if let Some(slot) = self.lookup(name) {
            let dst = self.temp(ty);
            self.emit(
                StmtKind::Load {
                    dst: Place::Local(dst),
                    src: Operand::Value(Value::Slot(slot)),
                },
                span,
            );
            return Value::Slot(dst);
        }

        if let Some(id) = self.default_of(name) {
            return Value::Slot(self.load_default(id, ty, span));
        }

        if let Some(id) = self.constants.get(name).copied() {
            return Value::Const(id);
        }

        if self.fn_index(name).is_some() {
            return self.constant(Const::Function(name.to_string()));
        }

        // A declared *type* used as a bare name, or a name the checker already rejected.
        // Lowering stays total and produces something harmless.
        self.constant(Const::None)
    }

    /// Copies a `default` into a slot, since a default is a place and not a value.
    fn load_default(&mut self, id: DefaultId, ty: Ty, span: Span) -> Slot {
        let dst = self.temp(ty);
        self.emit(
            StmtKind::Load {
                dst: Place::Local(dst),
                src: Operand::Default(id),
            },
            span,
        );
        dst
    }

    /// Concatenates a string literal's parts.
    fn string(&mut self, parts: &[StrPart]) -> Value {
        if let [StrPart::Literal { text, .. }] = parts {
            return self.constant(Const::Str(text.clone()));
        }

        let mut accumulator: Option<Value> = None;
        for part in parts {
            let piece = match part {
                StrPart::Literal { text, .. } => self.constant(Const::Str(text.clone())),
                StrPart::Interpolation { expr, span } => {
                    let value = self.expr(expr);
                    // `LANGUAGE.md §5.5` allows only displayable types here, so the only
                    // conversion needed is to text.
                    if self.type_of(expr) == Ty::Str {
                        value
                    } else {
                        let dst = self.temp(Ty::Str);
                        self.emit(
                            StmtKind::Call {
                                dst: Some(Place::Local(dst)),
                                callee: Callee::Direct(FuncRef::Builtin(Builtin::Str)),
                                args: vec![value],
                            },
                            *span,
                        );
                        Value::Slot(dst)
                    }
                }
            };

            accumulator = Some(match accumulator {
                None => piece,
                Some(previous) => {
                    let dst = self.temp(Ty::Str);
                    self.emit(
                        StmtKind::Assign {
                            dst: Place::Local(dst),
                            op: BinOp::Add,
                            a: previous,
                            b: piece,
                        },
                        part.span(),
                    );
                    Value::Slot(dst)
                }
            });
        }

        accumulator.unwrap_or_else(|| self.constant(Const::Str(String::new())))
    }
}
