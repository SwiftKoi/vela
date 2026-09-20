//! Expanding a `use`, and placing a caller's block (`SCREENS.md §2.1`).
//!
//! Composition is the one thing a screen body does that is a relationship between declarations rather
//! than a property of one line, so the state it needs travels together in [`Compose`] and the two
//! questions it answers live here: *which screen does this name mean, with which arguments* and *what
//! block may this body place*.
//!
//! The screen-name rule itself is [`crate::compose::find`] — one definition, shared with the checker,
//! the dependency walk, and the accessibility walk, so none of them can disagree with the drawing
//! about what a name refers to.

use vela_syntax::{ScreenArg, ScreenDecl, ScreenLine};

use crate::eval::{Args, value_of};

/// How far a `use` chain is expanded before the build stops.
///
/// The checker refuses a cycle (`E5011`), so this is a bound on a *crafted* pack rather than on a
/// file: a pack can be written by hand, and expanding `a` into `b` into `a` forever would be a hang
/// rather than a diagnostic.
const MAX_USE_DEPTH: usize = 32;

/// What a body may transclude, and how deep it already is.
#[derive(Clone, Copy, Default)]
pub(super) struct Compose<'a> {
    /// The block this body places where it writes `transclude`, and the scope it was written in.
    ///
    /// The scope is the *caller's*, not this body's: the block is the caller's code, so a name in it
    /// reads what the caller passed, which is what makes `use game_menu(title): text page_name` mean
    /// anything.
    pub(super) pane: Option<Pane<'a>>,
    /// How many `use` steps down this body is.
    pub(super) depth: usize,
    /// The style prefix in effect for this block, if any (`SCREENS.md §5.2`).
    ///
    /// Carried because it is a *scope*, like the pane: a block declares it once and every widget under
    /// it falls back to `{prefix}_{widget}`. A `use`d screen starts without one — a screen is a
    /// function, so it must not look different depending on who called it (`§2.1`).
    pub(super) prefix: Option<&'a str>,
}

/// A block handed to a `use`.
#[derive(Clone, Copy)]
pub(super) struct Pane<'a> {
    /// The lines to place.
    lines: &'a [ScreenLine],
    /// The scope they were written in.
    args: &'a Args,
}

impl<'a> Compose<'a> {
    /// The screen a `use` names, its arguments bound in *its* names, and the state to build its body
    /// with.
    ///
    /// `None` when the chain is already at its bound, which is the only way this refuses: an unknown
    /// screen name is nothing to expand, and the checker is what reports it.
    pub(super) fn expand(
        &self,
        screens: &'a [&'a ScreenDecl],
        name: &str,
        call: &'a [ScreenArg],
        caller: &'a Args,
        block: &'a [ScreenLine],
    ) -> Option<(&'a ScreenDecl, Args, Self)> {
        if self.depth >= MAX_USE_DEPTH {
            return None;
        }
        let callee = crate::compose::find(screens, name)?;
        let bound = bind(callee, call, caller);
        Some((
            callee,
            bound,
            Self {
                pane: Some(Pane {
                    lines: block,
                    args: caller,
                }),
                depth: self.depth + 1,
                // Not the caller's prefix: a screen's look is its own, or it would depend on where it
                // was used — the thing `§2.1`'s "a screen is a function" decision rules out.
                prefix: None,
            },
        ))
    }

    /// The block this body places, if it was handed one.
    pub(super) fn block(&self) -> Option<(&'a [ScreenLine], &'a Args)> {
        self.pane.map(|pane| (pane.lines, pane.args))
    }
}

/// The arguments a `use` binds, in the used screen's own parameter names.
///
/// Positional arguments bind in order and named ones by name; a parameter the call did not mention
/// keeps its default, which is [`crate::compose::with_defaults`] — the same rule an `open_screen` a
/// runtime performs follows, so the two ways a screen is called cannot drift apart.
fn bind(callee: &ScreenDecl, args: &[ScreenArg], caller: &Args) -> Args {
    // What the host says travels with the call: `variant(...)`, `setting(...)` and `slots(...)` must answer
    // the same in a *used* screen as in the caller, and a scope built from nothing would answer the defaults
    // — which is the one way a `use` could draw differently from the same screen opened by name. The slots
    // are the third of those, and the one this list forgot until the interface's own file screen was written
    // against a `use`: its cells all drew as empty, which is exactly the failure the comment above predicts.
    let mut bound = Args::new()
        .with_variants(caller.variants())
        .with_preferences(caller.preferences().clone())
        .with_slots(caller.slots().to_vec());
    let mut next = 0usize;

    for arg in args {
        match arg {
            ScreenArg::Value(expr) => {
                if let Some(param) = callee.params.get(next) {
                    bound.set(param.name.clone(), value_of(expr, caller));
                }
                next += 1;
            }
            ScreenArg::Named { name, value, .. } => {
                if let Some(value) = value {
                    bound.set(name.clone(), value_of(value, caller));
                }
            }
        }
    }

    crate::compose::with_defaults(callee, bound)
}
