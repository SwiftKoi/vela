//! Lowering the expressions that reach something else.
//!
//! A field read, a call, and a construction all have to answer the same question first:
//! what is this naming? `Ending.good` is a variant, `Ending` alone is a type, `route.name`
//! is a field, and the difference is decided once, here.

use vela_span::Span;
use vela_syntax::{Expr, Param};
use vela_types::{Ty, lower as lower_type};

use crate::ir::{
    Builtin, Callee, Const, FuncRef, Operand, Place, StmtKind, Symbol, Terminator, Value,
};
use crate::lower::Lowerer;

impl Lowerer<'_> {
    /// A field access: a variant, or a struct's field.
    pub(crate) fn field(&mut self, base: &Expr, name: &str, ty: Ty, span: Span) -> Value {
        // `Ending.good` names a variant rather than reading from a value. The distinction
        // is whether the base names a *type* — which is a name no local shadows.
        if let Expr::Name { name: path, .. } = base
            && self.lookup(path).is_none()
        {
            let base_ty = self.type_of(base);
            if let Ty::Enum(enum_name) = &base_ty {
                return self.constant(Const::Variant {
                    enum_name: enum_name.clone(),
                    variant: name.to_string(),
                });
            }
        }

        let base = self.expr(base);
        let dst = self.temp(ty);
        self.emit(
            StmtKind::Load {
                dst: Place::Local(dst),
                src: Operand::Field {
                    base,
                    field: name.to_string(),
                },
            },
            span,
        );
        Value::Slot(dst)
    }

    /// A call: construction, a conversion, a declared function, or a lambda.
    pub(crate) fn call(&mut self, callee: &Expr, args: &[Expr], ty: Ty, span: Span) -> Value {
        let values: Vec<Value> = args.iter().map(|arg| self.expr(arg)).collect();

        if let Some(value) = self.construct(callee, &values, span) {
            return value;
        }

        let target = self.target_of(callee);

        let dst = (ty != Ty::Unit).then(|| self.temp(ty));
        let callee = match target {
            Some(function) => Callee::Direct(function),
            None => Callee::Indirect(self.expr(callee)),
        };
        self.emit(
            StmtKind::Call {
                dst: dst.map(Place::Local),
                callee,
                args: values,
            },
            span,
        );

        match dst {
            Some(slot) => Value::Slot(slot),
            None => self.constant(Const::None),
        }
    }

    /// The statically-known target of a callee, if it has one.
    pub(crate) fn target_of(&mut self, callee: &Expr) -> Option<FuncRef> {
        // A dotted name that is a declared effect is a host call, checked before the
        // function table: an effect and a `fn` cannot share a name, and the dotted form is
        // what tells them apart.
        if let Some(full) = dotted(callee)
            && let Some(index) = self.effects.get(&full).copied()
        {
            return Some(FuncRef::Effect(index));
        }

        match callee {
            Expr::Name { name, .. } => Builtin::named(name)
                .map(FuncRef::Builtin)
                .or_else(|| self.fn_index(name).map(FuncRef::Defined)),
            Expr::Lambda { params, body, .. } => {
                Some(FuncRef::Defined(self.lift_lambda(params, body)))
            }
            _ => None,
        }
    }

    /// Builds a struct or an enum variant, if that is what is being called.
    pub(crate) fn construct(&mut self, callee: &Expr, args: &[Value], span: Span) -> Option<Value> {
        // A struct is constructed by calling its name: `Route("north", 3)`.
        if let Expr::Name { .. } = callee
            && let Ty::Struct(struct_name) = self.type_of(callee)
            && let Some(definition) = self.module.struct_named(&struct_name).cloned()
        {
            let fields: Vec<(String, Value)> = definition
                .fields
                .iter()
                .zip(args)
                .map(|(field, value)| (field.name.clone(), *value))
                .collect();
            let ty = self.type_of(callee);
            let dst = self.temp(ty);
            self.emit(
                StmtKind::StructNew {
                    dst: Place::Local(dst),
                    name: struct_name,
                    fields,
                },
                span,
            );
            return Some(Value::Slot(dst));
        }

        // A variant with a payload: `Ending.rewarded(5)`.
        if let Expr::Field { base, name, .. } = callee
            && let Expr::Name { name: path, .. } = base.as_ref()
            && self.lookup(path).is_none()
            && let Ty::Enum(enum_name) = self.type_of(base)
        {
            let dst = self.temp(Ty::Enum(enum_name.clone()));
            self.emit(
                StmtKind::EnumNew {
                    dst: Place::Local(dst),
                    enum_name,
                    variant: name.clone(),
                    args: args.to_vec(),
                },
                span,
            );
            return Some(Value::Slot(dst));
        }

        None
    }

    /// Lifts a lambda into a body of its own, returning its index.
    ///
    /// A lambda becomes an ordinary function. MIR has no closures, so a lambda that refers
    /// to an enclosing local would need an environment — which is why captured variables
    /// are resolved as *free* names, and a lambda that captures is a limitation rather
    /// than a feature at this milestone.
    pub(crate) fn lift_lambda(&mut self, params: &[Param], body: &Expr) -> u32 {
        let index = u32::try_from(self.module.fns.len()).unwrap_or(u32::MAX);
        let name = Symbol(format!("{}$lambda{index}", self.body.name()));

        let (lowered, ()) = self.in_body(name, Ty::Unit, |this| {
            for param in params {
                let ty = lower_type(&param.ty, this.env);
                let slot = this.declare(&param.name, ty);
                this.body.push_param(slot);
            }
            let value = this.expr(body);
            let ret = this.type_of(body);
            this.body.set_ret(ret.clone());
            let span = body.span();
            this.body
                .seal(Terminator::Return((ret != Ty::Unit).then_some(value)));
            let _ = span;
        });

        self.module.fns.push(lowered);
        index
    }

    /// The index of a declared function, for a direct call.
    pub(crate) fn fn_index(&self, name: &str) -> Option<u32> {
        self.funcs.get(name).copied()
    }
}

/// A dotted name as one string, when the expression is a chain of fields on a bare name.
///
/// `rand.int` and `route.name` have the same shape; which one it is depends on whether the
/// root is a local, and that question is the *checker's*. Here the effect table settles it:
/// a name that was declared is an effect, and anything else falls through to a field read.
fn dotted(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Name { name, .. } => Some(name.clone()),
        Expr::Field { base, name, .. } => Some(format!("{}.{name}", dotted(base)?)),
        _ => None,
    }
}
