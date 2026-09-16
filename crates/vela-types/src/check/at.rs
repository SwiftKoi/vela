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

/// The names in scope at `offset`, with their types.
///
/// The completion question, and the *other* half of what a position means: `at` answers "what is this
/// word", and this answers "what could be written here". Same walk, same scope rules — a second
/// traversal would answer the second question with a scope the first one does not use, which is how an
/// editor ends up offering a name the checker then rejects.
#[must_use]
pub fn scope_at(tree: &Program, env: &Env, offset: u32) -> Vec<(String, Ty)> {
    let mut checker = Checker::new(env, Scope::default()).collecting(offset);
    walk(&mut checker, tree);
    checker.names
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
    walk(&mut checker, tree);
    checker.answer
}

/// Walks every body in a module, which is what both questions above are asked of.
///
/// Shared rather than written twice: the two differ in what the checker is *recording*, not in which
/// bodies it visits, and two loops would drift the first time an item grew a body.
fn walk(checker: &mut Checker<'_>, tree: &Program) {
    for item in &tree.items {
        match item {
            Item::Label(decl) => {
                checker.scope = Scope::default();
                checker.body(&decl.body);
            }
            Item::Function(decl) => {
                checker.scope = Scope::default();
                for param in &decl.params {
                    let ty = crate::lower::lower(&param.ty, checker.env);
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
}

impl Checker<'_> {
    /// Starts answering instead of checking.
    pub(crate) fn asking(mut self, offset: u32) -> Self {
        self.ask = Some(offset);
        self
    }

    /// Starts collecting the scope at an offset instead of checking.
    pub(crate) fn collecting(mut self, offset: u32) -> Self {
        self.collect = Some(offset);
        self
    }

    /// Remembers the scope as of a position the walk has reached.
    ///
    /// The last snapshot at or before the offset wins, which is what makes the answer "the scope there"
    /// rather than "the scope somewhere": a `var` declared later is not offered, and one declared
    /// earlier is. Snapshotting once at the end would answer for the wrong line, and a completion that
    /// offers a name the checker has not introduced yet is how an author learns to distrust the list.
    pub(crate) fn snapshot(&mut self, upto: u32) {
        let Some(ask) = self.collect else {
            return;
        };
        if upto > ask {
            return;
        }
        self.names = self.scope.names();
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
