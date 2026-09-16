//! Typing expressions.

use vela_span::Span;
use vela_syntax::{Expr, Param, StrPart};

use crate::check::run::Checker;
use crate::error;
use crate::lower::lower;
use crate::ty::Ty;

impl Checker<'_> {
    /// Works out an expression's type, and notes it for a caller that asked about an offset.
    ///
    /// The wrapper exists because the work below returns early in half a dozen arms: recording at each
    /// of those returns would be a rule about this file's shape rather than about expressions, and the
    /// first arm added without one would silently stop answering.
    pub(crate) fn expr(&mut self, expr: &Expr) -> Ty {
        let ty = self.expr_uncached(expr);
        self.note(
            expr.span(),
            crate::check::at::Found::Expression { ty: ty.clone() },
        );
        ty
    }

    /// Works out an expression's type.
    ///
    /// Never fails. An expression that cannot be typed is `Unknown`, which fits everywhere
    /// and is never reported against — so one mistake produces one diagnostic rather than
    /// one for every place its result is used.
    fn expr_uncached(&mut self, expr: &Expr) -> Ty {
        match expr {
            Expr::Int { .. } => Ty::Int,
            Expr::Float { .. } => Ty::Float,
            Expr::Bool { .. } => Ty::Bool,
            Expr::None { .. } => Ty::None,
            // A path is an asset reference. Whether the asset exists is a build question
            // (`BUILD_AND_ASSETS.md §9`), not a typing one.
            Expr::Path { .. } => Ty::Unknown,
            Expr::Error { .. } => Ty::Unknown,

            Expr::Str { parts, .. } => {
                for part in parts {
                    if let StrPart::Interpolation { expr, span } = part {
                        let ty = self.expr(expr);
                        if !ty.is_displayable() {
                            self.report(error::not_displayable(&ty, *span));
                        }
                    }
                }
                Ty::Str
            }

            Expr::Name { name, .. } => self.name(name),
            Expr::Paren { inner, .. } => self.expr(inner),

            Expr::List { items, .. } => {
                let element = items.first().map_or(Ty::Unknown, |first| self.expr(first));
                for item in items.iter().skip(1) {
                    let other = self.expr(item);
                    if !element.accepts(&other) {
                        self.report(error::mismatch(&element, &other, item.span()));
                    }
                }
                Ty::List(Box::new(element))
            }

            Expr::Map { entries, .. } => self.map(entries),
            Expr::Field { base, name, .. } => self.field(base, name),
            Expr::Index { base, index, .. } => self.index(base, index),
            Expr::Call { callee, args, .. } => self.call_(callee, args),
            Expr::Unary { op, operand, span } => self.unary(*op, operand, *span),
            Expr::Binary { op, lhs, rhs, span } => {
                let left = self.expr(lhs);
                let right = self.expr(rhs);
                self.binary(*op, &left, &right, *span)
            }

            Expr::If {
                cond,
                then_,
                else_,
                span,
            } => self.conditional(cond, then_, else_, *span),
            Expr::Lambda { params, body, .. } => self.lambda(params, body),
        }
    }

    /// The type of a name in scope.
    fn name(&mut self, name: &str) -> Ty {
        if let Some(ty) = self.scope.get(name) {
            return ty.clone();
        }
        // A name that is not here has already been reported as undefined; reporting a type
        // problem as well would be a second error for one mistake.
        self.env.value(name).cloned().unwrap_or(Ty::Unknown)
    }

    /// The type of a field access, which is a variant, a struct field, or a name in its own
    /// right.
    fn field(&mut self, base: &Expr, name: &str) -> Ty {
        if let Expr::Name {
            name: base_name, ..
        } = base
            && let Some(Ty::Enum(enum_name)) = self.env.value(base_name).cloned()
        {
            // `Ending.good` names a variant, and its type is the enum itself.
            return Ty::Enum(enum_name);
        }

        // `rand.int` is one declared name, not `int` inside `rand`. Checked before the
        // struct path because a dotted name whose root holds no value cannot be a field
        // access — there is nothing to take a field of.
        if let Some(full) = dotted(base, name)
            && let Some((root, _)) = full.split_once('.')
            && self.scope.get(root).is_none()
            && let Some(ty) = self.env.value(&full).cloned()
        {
            return ty;
        }

        let base_ty = self.expr(base);
        if let Ty::Struct(struct_name) = &base_ty
            && let Some(field) = self.env.struct_field(struct_name, name)
        {
            return field.clone();
        }
        Ty::Unknown
    }

    /// The type of a `map` literal, taking its shape from the first entry.
    fn map(&mut self, entries: &[(Expr, Expr)]) -> Ty {
        let key = entries
            .first()
            .map_or(Ty::Unknown, |(key, _)| self.expr(key));
        let value = entries
            .first()
            .map_or(Ty::Unknown, |(_, value)| self.expr(value));
        for (extra_key, extra_value) in entries.iter().skip(1) {
            self.expr(extra_key);
            self.expr(extra_value);
        }
        Ty::Map(Box::new(key), Box::new(value))
    }

    /// The type of an index expression.
    fn index(&mut self, base: &Expr, index: &Expr) -> Ty {
        let base = self.expr(base);
        self.expr(index);
        match base {
            Ty::List(element) => *element,
            Ty::Map(_, value) => *value,
            _ => Ty::Unknown,
        }
    }

    /// The type of a call.
    ///
    /// Arguments are checked against the parameters the callee was declared with, which
    /// covers `fn`s and effects alike — they are the same thing to a caller and differ only
    /// in who implements them.
    fn call_(&mut self, callee: &Expr, args: &[Expr]) -> Ty {
        let mut found = Vec::with_capacity(args.len());
        for argument in args {
            found.push(self.expr(argument));
        }

        // The conversions are the few names the language provides, so they are recognised
        // before anything else claims them. Each takes exactly one argument.
        if let Expr::Name { name, .. } = callee
            && let Some(ty) = builtin(name)
        {
            if args.len() != 1 {
                self.report(error::wrong_arity(1, args.len(), callee.span()));
            }
            return ty;
        }

        match self.expr(callee) {
            Ty::Fn(params, ret) => {
                self.check_arguments(&params, &found, args, callee.span());
                *ret
            }
            _ => Ty::Unknown,
        }
    }

    /// Checks a call's arguments against the parameters it was declared with.
    fn check_arguments(&mut self, params: &[Ty], found: &[Ty], args: &[Expr], span: Span) {
        if params.len() != found.len() {
            self.report(error::wrong_arity(params.len(), found.len(), span));
            return;
        }
        for ((expected, found), argument) in params.iter().zip(found).zip(args) {
            if !expected.accepts(found) {
                self.report(error::mismatch(expected, found, argument.span()));
            }
        }
    }

    /// The type of a conditional expression.
    fn conditional(&mut self, cond: &Expr, then_: &Expr, else_: &Expr, span: Span) -> Ty {
        let condition = self.expr(cond);
        if condition != Ty::Bool && condition != Ty::Unknown {
            self.report(error::mismatch(&Ty::Bool, &condition, cond.span()));
        }

        let then_ty = self.expr(then_);
        let else_ty = self.expr(else_);
        // `LANGUAGE.md §5.4`: inference never widens across branches, so a
        // conditional whose arms disagree is reported rather than guessed at.
        if !then_ty.accepts(&else_ty) && !else_ty.accepts(&then_ty) {
            self.report(error::branch_mismatch(&then_ty, &else_ty, span));
            return Ty::Unknown;
        }
        then_ty
    }

    /// The type of a lambda.
    fn lambda(&mut self, params: &[Param], body: &Expr) -> Ty {
        let saved = self.scope.snapshot();
        let param_types: Vec<Ty> = params
            .iter()
            .map(|param| lower(&param.ty, self.env))
            .collect();
        for (param, ty) in params.iter().zip(&param_types) {
            self.scope.insert(param.name.clone(), ty.clone());
        }
        let ret = self.expr(body);
        self.scope.restore(saved);
        Ty::Fn(param_types, Box::new(ret))
    }
}

/// A field chain as one dotted name, with a further segment appended.
///
/// `rand.int` and `route.name` have the same shape, and which one it is depends on whether
/// the root is something that holds a value: a local means a field access, and anything else
/// means a name that was declared whole.
fn dotted(expr: &Expr, name: &str) -> Option<String> {
    Some(format!("{}.{name}", chain(expr)?))
}

/// The dotted name a pure field chain spells, when its root is a bare name.
fn chain(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Name { name, .. } => Some(name.clone()),
        Expr::Field { base, name, .. } => Some(format!("{}.{name}", chain(base)?)),
        _ => None,
    }
}

/// The type a conversion function produces.
fn builtin(name: &str) -> Option<Ty> {
    match name {
        "str" => Some(Ty::Str),
        "int" => Some(Ty::Int),
        "float" => Some(Ty::Float),
        "bool" => Some(Ty::Bool),
        _ => None,
    }
}
