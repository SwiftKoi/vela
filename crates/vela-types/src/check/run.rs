//! The checking entry point and the state it carries.

use vela_diag::Diagnostic;
use vela_span::Span;
use vela_syntax::{Expr, Item, Program};

use crate::env::{Env, Scope};
use crate::error;
use crate::lower::lower;
use crate::ty::Ty;

/// Checks every body in a module against its declarations.
#[must_use]
pub fn check(tree: &Program, env: &Env) -> Vec<Diagnostic> {
    let mut checker = Checker {
        env,
        scope: Scope::default(),
        diagnostics: Vec::new(),
    };

    for item in &tree.items {
        match item {
            Item::Label(decl) => {
                // Each body starts with only its own names in scope.
                checker.scope = Scope::default();
                checker.body(&decl.body);
            }
            Item::Function(decl) => {
                checker.scope = Scope::default();
                for param in &decl.params {
                    let ty = lower(&param.ty, env);
                    checker.scope.insert(param.name.clone(), ty);
                }
                // Whether every path through the body exits comes back from the walk itself,
                // which is the only place the scope needed to type a `match`'s scrutinee exists.
                let exits = checker.body(&decl.body);

                // A function that promises a value has to produce one on every path.
                if decl.ret.is_some() && !exits {
                    let ret = decl.ret.as_ref().map_or(Ty::Unit, |ty| lower(ty, env));
                    checker.report(error::missing_return(&decl.name, decl.span, &ret));
                }
            }
            _ => {}
        }
    }

    checker.diagnostics
}

/// The type of an expression, in a scope.
///
/// Exactly what the checker computes, exposed so that lowering can annotate the MIR it
/// builds without a second opinion about what an expression's type is. Two implementations
/// of that question would agree until the first one was fixed, and then MIR would be typed
/// against a rule the checker no longer used.
///
/// Diagnostics are discarded: `check` has already reported them, and asking again would
/// report each one twice.
#[must_use]
pub fn type_of(env: &Env, scope: &Scope, expr: &Expr) -> Ty {
    let mut checker = Checker {
        env,
        scope: scope.snapshot(),
        diagnostics: Vec::new(),
    };
    checker.expr(expr)
}

/// The state of one module's check.
pub(crate) struct Checker<'a> {
    /// The module's declarations.
    pub(crate) env: &'a Env,
    /// The names the current body has introduced.
    pub(crate) scope: Scope,
    /// Problems found so far.
    pub(crate) diagnostics: Vec<Diagnostic>,
}

impl Checker<'_> {
    /// Records a problem.
    pub(crate) fn report(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// Checks a value against what is expected of it.
    ///
    /// An optional where its payload is wanted gets its own message. "Unwrap it" is
    /// actionable where "type mismatch" is not, and it is by far the common case — using a
    /// `T?` as a `T` is what forgetting a `??` looks like.
    pub(crate) fn expect(&mut self, expected: &Ty, found: &Ty, span: Span) {
        if expected.accepts(found) {
            return;
        }
        if found.is_optional() {
            self.report(error::needs_unwrap(expected, span));
            return;
        }
        self.report(error::mismatch(expected, found, span));
    }
}
