//! Lowering declarations.

use std::collections::BTreeMap;

use vela_syntax::{BinOp, UnOp};
use vela_syntax::{ConstDecl, DefaultDecl, EnumDecl, Expr, FnDecl, LabelDecl, StrPart, StructDecl};
use vela_types::{Ty, lower as lower_type};

use crate::ir::{
    Const, ConstDef, ConstId, ConstPool, DefaultDef, EnumDef, FieldDef, StructDef, Symbol,
    VariantDef,
};
use crate::lower::Lowerer;

impl Lowerer<'_> {
    /// Lowers a declared struct.
    pub(crate) fn lower_struct(&mut self, decl: &StructDecl) {
        let fields = decl
            .fields
            .iter()
            .map(|field| FieldDef {
                name: field.name.clone(),
                ty: lower_type(&field.ty, self.env),
            })
            .collect();

        self.module.structs.push(StructDef {
            name: decl.name.clone(),
            fields,
        });
    }

    /// Lowers a declared enum.
    pub(crate) fn lower_enum(&mut self, decl: &EnumDecl) {
        let variants = decl
            .variants
            .iter()
            .map(|variant| VariantDef {
                name: variant.name.clone(),
                fields: variant
                    .fields
                    .iter()
                    .map(|field| lower_type(&field.ty, self.env))
                    .collect(),
            })
            .collect();

        self.module.enums.push(EnumDef {
            name: decl.name.clone(),
            variants,
        });
    }

    /// Lowers a `const`.
    ///
    /// `LANGUAGE.md §7.2` gives no rule for what an initialiser may be, but a `const` that
    /// is not constant is a contradiction: it has to be a value the compiler has, because
    /// every use of it is replaced by that value.
    pub(crate) fn lower_const(&mut self, decl: &ConstDecl) {
        let ty = decl
            .ty
            .as_ref()
            .map_or(Ty::Unknown, |written| lower_type(written, self.env));

        let value = fold(&decl.value, &self.constants, &self.module.pool);
        let Some(value) = value else {
            self.diagnostics
                .push(crate::error::not_constant(&decl.name, decl.value.span()));
            return;
        };

        let id = self.module.pool.add(value);
        self.constants.insert(decl.name.clone(), id);
        self.module.consts.push(ConstDef {
            name: decl.name.clone(),
            ty,
            value: id,
        });
    }

    /// Lowers a `default`.
    ///
    /// A `default` is world state and part of the save schema (`RUNTIME.md §5`). Its
    /// initial value has to be known at compile time for that schema to be derivable from
    /// the source, which is the whole point of `default` existing rather than `global`.
    pub(crate) fn lower_default(&mut self, decl: &DefaultDecl) {
        let ty = decl
            .ty
            .as_ref()
            .map_or(Ty::Unknown, |written| lower_type(written, self.env));

        let value = fold(&decl.value, &self.constants, &self.module.pool);
        let Some(value) = value else {
            self.diagnostics
                .push(crate::error::not_constant(&decl.name, decl.value.span()));
            return;
        };

        let id = self.module.pool.add(value);
        self.constants.insert(decl.name.clone(), id);
        self.module.defaults.push(DefaultDef {
            name: decl.name.clone(),
            ty,
            init: id,
        });
    }

    /// Lowers an effect declaration.
    ///
    /// One line of work, and that is the design showing through: an effect has no body to
    /// lower. The declaration exists so a *call* can resolve to a host capability rather
    /// than to a function, and the runtime supplies the rest.
    pub(crate) fn lower_effect(&mut self, decl: &vela_syntax::EffectDecl) {
        let params = decl
            .params
            .iter()
            .map(|param| lower_type(&param.ty, self.env))
            .collect();
        let ret = decl
            .ret
            .as_ref()
            .map_or(Ty::Unit, |written| lower_type(written, self.env));

        self.module.effects.push(crate::ir::EffectDef {
            name: decl.dotted(),
            params,
            ret,
        });
    }

    /// Lowers a function.
    pub(crate) fn lower_function(&mut self, decl: &FnDecl) {
        let ret = decl
            .ret
            .as_ref()
            .map_or(Ty::Unit, |written| lower_type(written, self.env));
        let name = Symbol(decl.name.clone());

        let (body, ()) = self.in_body(name, ret, |this| {
            for param in &decl.params {
                let ty = lower_type(&param.ty, this.env);
                let slot = this.declare(&param.name, ty);
                this.body.push_param(slot);
            }
            this.statements(&decl.body);
        });

        self.module.fns.push(body);
    }

    /// Lowers a label.
    ///
    /// A label returns nothing, even though `return` inside one ends it: a label is a
    /// story node, and the value of "leaving" it is not a value the story can use.
    pub(crate) fn lower_label(&mut self, decl: &LabelDecl) {
        let name = Symbol(decl.name.clone());
        let (body, ()) = self.in_body(name, Ty::Unit, |this| {
            this.statements(&decl.body);
        });
        self.module.labels.push(body);
    }
}

/// A constant expression, folded.
///
/// Handles literals, arithmetic over them, string concatenation, the conversions, and
/// references to constants already folded. That covers every declaration in `LANGUAGE.md
/// §7`, and anything outside it is reported rather than silently initialised to nothing.
#[must_use]
pub fn fold(expr: &Expr, known: &BTreeMap<String, ConstId>, pool: &ConstPool) -> Option<Const> {
    match expr {
        Expr::Int { value, .. } => Some(Const::Int(*value)),
        Expr::Float { value, .. } => Some(Const::Float(*value)),
        Expr::Bool { value, .. } => Some(Const::Bool(*value)),
        Expr::None { .. } => Some(Const::None),
        Expr::Path { value, .. } => Some(Const::Str(value.clone())),
        Expr::Str { parts, .. } => fold_string(parts, known, pool),
        Expr::Name { name, .. } => known.get(name).and_then(|id| pool.get(*id)).cloned(),
        Expr::Paren { inner, .. } => fold(inner, known, pool),
        Expr::Unary { op, operand, .. } => {
            let value = fold(operand, known, pool)?;
            fold_unary(*op, value)
        }
        Expr::Binary { op, lhs, rhs, .. } => {
            let left = fold(lhs, known, pool)?;
            let right = fold(rhs, known, pool)?;
            fold_binary(*op, left, right)
        }
        Expr::Call { callee, args, .. } => {
            let Expr::Name { name, .. } = callee.as_ref() else {
                return None;
            };
            let [argument] = args.as_slice() else {
                return None;
            };
            let value = fold(argument, known, pool)?;
            fold_conversion(name, value)
        }
        _ => None,
    }
}

/// Folds a string literal, which may interpolate constants.
fn fold_string(
    parts: &[StrPart],
    known: &BTreeMap<String, ConstId>,
    pool: &ConstPool,
) -> Option<Const> {
    let mut text = String::new();
    for part in parts {
        match part {
            StrPart::Literal { text: literal, .. } => text.push_str(literal),
            StrPart::Interpolation { expr, .. } => {
                let value = fold(expr, known, pool)?;
                text.push_str(&render(&value));
            }
        }
    }
    Some(Const::Str(text))
}

/// Folds a prefix operation.
pub(crate) fn fold_unary(op: UnOp, value: Const) -> Option<Const> {
    match (op, value) {
        (UnOp::Neg, Const::Int(number)) => Some(Const::Int(-number)),
        (UnOp::Neg, Const::Float(number)) => Some(Const::Float(-number)),
        (UnOp::Not | UnOp::Bang, Const::Bool(flag)) => Some(Const::Bool(!flag)),
        _ => None,
    }
}

/// Folds a binary operation.
pub(crate) fn fold_binary(op: BinOp, left: Const, right: Const) -> Option<Const> {
    match (op, &left, &right) {
        (BinOp::Add, Const::Int(a), Const::Int(b)) => Some(Const::Int(a.wrapping_add(*b))),
        (BinOp::Sub, Const::Int(a), Const::Int(b)) => Some(Const::Int(a.wrapping_sub(*b))),
        (BinOp::Mul, Const::Int(a), Const::Int(b)) => Some(Const::Int(a.wrapping_mul(*b))),
        (BinOp::Div, Const::Int(a), Const::Int(b)) if *b != 0 => Some(Const::Int(a / b)),
        (BinOp::Rem, Const::Int(a), Const::Int(b)) if *b != 0 => Some(Const::Int(a % b)),

        (BinOp::Add, Const::Float(a), Const::Float(b)) => Some(Const::Float(a + b)),
        (BinOp::Sub, Const::Float(a), Const::Float(b)) => Some(Const::Float(a - b)),
        (BinOp::Mul, Const::Float(a), Const::Float(b)) => Some(Const::Float(a * b)),
        (BinOp::Div, Const::Float(a), Const::Float(b)) => Some(Const::Float(a / b)),

        // `+` on two strings is concatenation, which is how interpolation lowers.
        (BinOp::Add, Const::Str(a), Const::Str(b)) => Some(Const::Str(format!("{a}{b}"))),

        (BinOp::Eq, a, b) => Some(Const::Bool(a == b)),
        (BinOp::Ne, a, b) => Some(Const::Bool(a != b)),
        (BinOp::And, Const::Bool(a), Const::Bool(b)) => Some(Const::Bool(*a && *b)),
        (BinOp::Or, Const::Bool(a), Const::Bool(b)) => Some(Const::Bool(*a || *b)),
        _ => None,
    }
}

/// Folds a conversion.
fn fold_conversion(name: &str, value: Const) -> Option<Const> {
    match name {
        "str" => Some(Const::Str(render(&value))),
        "int" => match value {
            Const::Int(_) => Some(value),
            Const::Float(number) => Some(Const::Int(number as i64)),
            Const::Bool(flag) => Some(Const::Int(i64::from(flag))),
            _ => None,
        },
        "float" => match value {
            Const::Int(number) => Some(Const::Float(number as f64)),
            Const::Float(_) => Some(value),
            _ => None,
        },
        "bool" => match value {
            Const::Bool(_) => Some(value),
            _ => None,
        },
        _ => None,
    }
}

/// Renders a folded constant, for `str(…)` and for interpolation.
fn render(value: &Const) -> String {
    match value {
        Const::None => "none".to_string(),
        Const::Bool(flag) => flag.to_string(),
        Const::Int(number) => number.to_string(),
        Const::Float(number) => vela_world::format_float(*number),
        Const::Str(text) => text.clone(),
        Const::Variant { enum_name, variant } => format!("{enum_name}.{variant}"),
        Const::Function(name) => name.clone(),
    }
}
