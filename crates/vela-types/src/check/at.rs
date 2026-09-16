//! What the checker knows about a position in a file.
//!
//! Hover, and the "insert the type here" family of quick fixes, ask one question: what is this, and
//! what type does it have? The answer is a *mode of the checker* rather than a second walk. A separate
//! traversal would have to re-derive the scope — which `var` is visible where — and that is exactly the
//! rule that would drift: the checker's scope is deliberately flat and laxer than the language, so a
//! second, stricter walk would call a name unknown where the checker accepts it, and the editor would
//! contradict the very diagnostics it is showing.

use vela_span::Span;
use vela_syntax::{Item, Program};

use crate::check::run::Checker;
use crate::env::{Env, Scope};
use crate::ty::Ty;

/// What a position in a file names.
#[derive(Clone, PartialEq, Debug)]
pub enum Found {
    /// A name a body introduces — a local or a parameter — and its type.
    Name {
        /// The name as written.
        name: String,
        /// Its type, inferred from the initialiser or declared.
        ty: Ty,
    },
    /// An expression, and the type this module gives it.
    Expression {
        /// The type.
        ty: Ty,
    },
}

impl Found {
    /// The type of what was found, however it was found.
    #[must_use]
    pub fn ty(&self) -> &Ty {
        match self {
            Self::Name { ty, .. } | Self::Expression { ty } => ty,
        }
    }

    /// What to call it in a sentence: a local's name, or "a value" for an expression.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Name { name, ty } => format!("{name}: {ty}"),
            Self::Expression { ty } => ty.to_string(),
        }
    }
}

/// What the checker can say about `offset` in a module, if it says anything.
///
/// Diagnostics are discarded, for the reason `type_of` gives: `check` has already reported them, and a
/// caller asking what sits at a position is not asking for a second opinion about the file.
///
/// The *innermost* thing containing the offset wins — `route.name` answers about `route.name` rather
/// than about the call it appears in — which is what a reader pointing at a word means.
#[must_use]
pub fn at(tree: &Program, env: &Env, offset: u32) -> Option<Found> {
    let mut checker = Checker::new(env, Scope::default()).asking(offset);

    for item in &tree.items {
        match item {
            Item::Label(decl) => {
                checker.scope = Scope::default();
                checker.body(&decl.body);
            }
            Item::Function(decl) => {
                checker.scope = Scope::default();
                for param in &decl.params {
                    let ty = crate::lower::lower(&param.ty, env);
                    // A parameter's own span, which the tree does record: hovering `trust` in
                    // `fn spend(trust: int)` says `trust: int`.
                    checker.note(
                        param.span,
                        Found::Name {
                            name: param.name.clone(),
                            ty: ty.clone(),
                        },
                    );
                    checker.scope.insert(param.name.clone(), ty);
                }
                checker.body(&decl.body);
            }
            _ => {}
        }
    }

    checker.answer
}

impl Checker<'_> {
    /// Starts answering instead of checking.
    pub(crate) fn asking(mut self, offset: u32) -> Self {
        self.ask = Some(offset);
        self
    }

    /// Records what is at the offset, keeping the innermost answer.
    ///
    /// "Innermost" by span containment rather than by call order: the checker types an expression before
    /// its operands in some cases and after in others, so relying on the order would make the answer
    /// depend on which arm of a `match` happened to run first.
    pub(crate) fn note(&mut self, span: Span, found: Found) {
        let Some(ask) = self.ask else {
            return;
        };
        if span.start() > ask || ask >= span.end() {
            return;
        }
        if let Some(best) = self.best
            && !(span.start() >= best.start() && span.end() <= best.end())
        {
            return;
        }

        self.best = Some(span);
        self.answer = Some(found);
    }
}
