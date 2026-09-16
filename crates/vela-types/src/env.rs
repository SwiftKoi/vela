//! The type environment: what a module's declarations make available.
//!
//! Built by walking a module's declarations, so that checking a body is a lookup rather
//! than a search. Only types live here — names are `vela-hir`'s job, and the two are
//! separate passes because a name can be perfectly well defined and still hold the wrong
//! type.

use std::collections::BTreeMap;

use vela_diag::Diagnostic;
use vela_syntax::{Expr, Item, Program, Type};

use crate::error;
use crate::lower::lower;
use crate::ty::Ty;

/// An enum's variants, with their payloads.
#[derive(Debug, Default)]
pub struct EnumShape {
    /// Variant name to its payload types; `None` is a variant with no payload.
    pub variants: BTreeMap<String, Option<Vec<Ty>>>,
}

/// What a module's declarations provide.
#[derive(Debug, Default)]
pub struct Env {
    /// Every name that holds a value, and its type.
    values: BTreeMap<String, Ty>,
    /// Struct name to its fields, in declaration order.
    ///
    /// Names are kept, not just types: `route.name` has to find `name` among them.
    structs: BTreeMap<String, Vec<(String, Ty)>>,
    /// Enum name to its shape.
    enums: BTreeMap<String, EnumShape>,
}

impl Env {
    /// Builds the environment from a module's declarations.
    ///
    /// Two passes, because a field may name a type declared further down the file: pass one
    /// registers every declared name, pass two lowers the types that refer to them.
    pub fn build(tree: &Program) -> (Self, Vec<Diagnostic>) {
        let mut env = Self::default();
        let mut diagnostics = Vec::new();

        env.register(tree, &mut diagnostics);
        env.fill(tree, &mut diagnostics);

        (env, diagnostics)
    }

    /// Pass one: register every declared name.
    ///
    /// Before anything is lowered, so a field may name a type declared further down the
    /// file — and so a struct and an enum cannot both claim a name without one of them
    /// quietly losing.
    fn register(&mut self, tree: &Program, diagnostics: &mut Vec<Diagnostic>) {
        for item in &tree.items {
            match item {
                Item::Struct(decl) => {
                    self.values
                        .insert(decl.name.clone(), Ty::Struct(decl.name.clone()));
                    self.structs.entry(decl.name.clone()).or_default();
                }
                Item::Enum(decl) => {
                    if decl.variants.is_empty() {
                        diagnostics.push(error::empty_enum(&decl.name, decl.span));
                    }
                    self.values
                        .insert(decl.name.clone(), Ty::Enum(decl.name.clone()));
                    self.enums.entry(decl.name.clone()).or_default();
                }
                _ => {}
            }
        }
    }

    /// Pass two: lower every written type, now that every declared name is known.
    fn fill(&mut self, tree: &Program, diagnostics: &mut Vec<Diagnostic>) {
        for item in &tree.items {
            match item {
                Item::Struct(decl) => {
                    // The environment is borrowed immutably to lower, then mutably to
                    // store; naming the borrow keeps the two from overlapping.
                    let fields: Vec<(String, Ty)> = {
                        let env: &Self = self;
                        decl.fields
                            .iter()
                            .map(|field| (field.name.clone(), lower(&field.ty, env)))
                            .collect()
                    };
                    self.structs.insert(decl.name.clone(), fields);
                }
                Item::Enum(decl) => {
                    let mut shape = EnumShape::default();
                    for variant in &decl.variants {
                        let payload = if variant.fields.is_empty() {
                            None
                        } else {
                            let env: &Self = self;
                            Some(
                                variant
                                    .fields
                                    .iter()
                                    .map(|field| lower(&field.ty, env))
                                    .collect(),
                            )
                        };
                        shape.variants.insert(variant.name.clone(), payload);
                    }
                    self.enums.insert(decl.name.clone(), shape);
                }
                Item::Const(decl) => {
                    let ty =
                        self.declared_value(decl.ty.as_ref(), &decl.value, &decl.name, diagnostics);
                    self.values.insert(decl.name.clone(), ty);
                }
                Item::Default(decl) => {
                    let ty =
                        self.declared_value(decl.ty.as_ref(), &decl.value, &decl.name, diagnostics);
                    self.values.insert(decl.name.clone(), ty);
                }
                // Registered under its *dotted* name, because that is how a call names it:
                // two capabilities can share a leaf (`audio.position`, `input.position`).
                Item::Effect(decl) => {
                    let (params, ret) = {
                        let env: &Self = self;
                        let params: Vec<Ty> =
                            decl.params.iter().map(|p| lower(&p.ty, env)).collect();
                        let ret = decl.ret.as_ref().map_or(Ty::Unit, |ty| lower(ty, env));
                        (params, ret)
                    };
                    self.values
                        .insert(decl.dotted(), Ty::Fn(params, Box::new(ret)));
                }
                Item::Function(decl) => {
                    let (params, ret) = {
                        let env: &Self = self;
                        let params: Vec<Ty> =
                            decl.params.iter().map(|p| lower(&p.ty, env)).collect();
                        let ret = decl.ret.as_ref().map_or(Ty::Unit, |ty| lower(ty, env));
                        (params, ret)
                    };
                    self.values
                        .insert(decl.name.clone(), Ty::Fn(params, Box::new(ret)));
                }
                _ => {}
            }
        }
    }

    /// The type a `const` or `default` declares, or fails to.
    ///
    /// An unwritten type is only acceptable when the initialiser cannot be misread, which
    /// is why a call needs an annotation: its result is a fact about somewhere else, and a
    /// declaration that does not say what it holds will change meaning when that somewhere
    /// else does.
    fn declared_value(
        &self,
        annotation: Option<&Type>,
        value: &Expr,
        name: &str,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Ty {
        match annotation {
            Some(annotation) => lower(annotation, self),
            None if is_unambiguous(value) => Ty::Unknown,
            None => {
                diagnostics.push(error::needs_annotation(name, value.span()));
                Ty::Unknown
            }
        }
    }

    /// The type a declared name stands for.
    #[must_use]
    pub fn declared(&self, name: &str) -> Option<Ty> {
        if self.structs.contains_key(name) {
            return Some(Ty::Struct(name.to_string()));
        }
        self.enums
            .contains_key(name)
            .then(|| Ty::Enum(name.to_string()))
    }

    /// The type of a name that holds a value.
    #[must_use]
    pub fn value(&self, name: &str) -> Option<&Ty> {
        self.values.get(name)
    }

    /// A struct's fields, in declaration order.
    #[must_use]
    pub fn struct_fields(&self, name: &str) -> Option<&[(String, Ty)]> {
        self.structs.get(name).map(Vec::as_slice)
    }

    /// The type of one of a struct's fields.
    #[must_use]
    pub fn struct_field(&self, name: &str, field: &str) -> Option<&Ty> {
        self.structs
            .get(name)?
            .iter()
            .find(|(candidate, _)| candidate == field)
            .map(|(_, ty)| ty)
    }

    /// An enum's shape.
    #[must_use]
    pub fn enum_shape(&self, name: &str) -> Option<&EnumShape> {
        self.enums.get(name)
    }
}

/// Whether an initialiser's type is obvious from the expression alone.
fn is_unambiguous(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Int { .. }
            | Expr::Float { .. }
            | Expr::Str { .. }
            | Expr::Bool { .. }
            | Expr::None { .. }
            | Expr::Path { .. }
            | Expr::List { .. }
            | Expr::Map { .. }
            | Expr::Name { .. }
            | Expr::Paren { .. }
    )
}

/// The names a body has introduced, and their types.
///
/// Flat rather than block-scoped, matching `vela-hir`'s resolver: two passes disagreeing
/// about what is in scope would produce a name that resolves and then does not type.
#[derive(Debug, Default)]
pub struct Scope {
    names: BTreeMap<String, Ty>,
}

impl Scope {
    /// The type of a local, if it is one.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Ty> {
        self.names.get(name)
    }

    /// Introduces a local, and its type.
    pub fn insert(&mut self, name: impl Into<String>, ty: Ty) {
        self.names.insert(name.into(), ty);
    }

    /// Every name in scope, in order.
    ///
    /// For completion, which asks what could be typed here rather than what one name means. Ordered by
    /// name, so two runs offer the same list in the same order — which is what keeps a completion popup
    /// from reshuffling under the cursor.
    #[must_use]
    pub fn names(&self) -> Vec<(String, Ty)> {
        self.names
            .iter()
            .map(|(name, ty)| (name.clone(), ty.clone()))
            .collect()
    }

    /// A copy, for saving before a nested scope.
    #[must_use]
    pub fn snapshot(&self) -> Self {
        Self {
            names: self.names.clone(),
        }
    }

    /// Restores one saved by [`Scope::snapshot`].
    pub fn restore(&mut self, saved: Self) {
        *self = saved;
    }
}
