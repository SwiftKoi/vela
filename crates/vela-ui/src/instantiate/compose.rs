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
    pane: Option<Pane<'a>>,
    /// How many `use` steps down this body is.
    depth: usize,
}

/// A block handed to a `use`.
#[derive(Clone, Copy)]
struct Pane<'a> {
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
/// keeps its default. A parameter with neither is `none` — the checker has already reported the
/// mismatch, and a screen somebody is midway through editing should draw something rather than
/// nothing.
fn bind(callee: &ScreenDecl, args: &[ScreenArg], caller: &Args) -> Args {
    let mut bound = Args::new();
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

    // Defaults last, and evaluated against what is already bound, so a default may name a parameter
    // that *was* passed rather than only a literal.
    for param in &callee.params {
        if bound.get(&param.name).is_some() {
            continue;
        }
        let Some(default) = &param.default else {
            continue;
        };
        let value = value_of(default, &bound);
        bound.set(param.name.clone(), value);
    }
    bound
}
