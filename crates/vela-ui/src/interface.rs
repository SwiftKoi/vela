//! The screens Vela ships: the interface a game has before it writes one (`SCREENS.md §2.1`).
//!
//! One module, one source file, compiled like a project's — the whole point of M12.2's second decision:
//! *a default interface, and a project overrides any part of it*. What makes it a module rather than a
//! list of names is the same thing that makes a project's screens work: the interface's screens `use`
//! each other, so the frame an app screen is built on is written once.
//!
//! **The resolution rule is one line in one place.** [`crate::compose::find`] is the single definition of
//! "which screen does this name mean", shared by the checker, the instantiator, the dependency walk, and
//! the accessibility walk — and it falls back to these declarations when the file being checked does not
//! declare the name. So a project module can `use game_menu(...)` and an `open_screen("…")` can name one
//! of these screens, with no second table anywhere.
//!
//! What an override *is* follows from §2.1's "a screen is a pure function of its arguments": the
//! project's declaration takes over the **name**, and a screen the interface uses internally keeps using
//! its own — an override replaces a screen, not a binding. Otherwise the interface's `preferences` would
//! draw differently depending on what the project happened to declare, which is exactly the coupling the
//! screen language rules out everywhere else.
//!
//! Parsed once, on first use. It is the engine's own source: a built bundle carries the *project's*
//! screens (`§13`) and no interface, because the interface belongs to the engine that is running rather
//! than to the artifact that was built.

use std::sync::OnceLock;

use vela_syntax::{Item, ScreenDecl, parse};

use crate::screens::ScreenSet;

/// The interface's source, compiled into this build.
pub const SOURCE: &str = include_str!("../interface.vela");

/// The parsed interface.
///
/// A parse rather than a baked artifact, because the engine's own interface should be reviewable as
/// source and cannot drift from the language it is written in — and because it happens once per process
/// (`§13`'s property is about a *bundle*: no project source is parsed at run time, and this is not a
/// project's).
fn parsed() -> &'static vela_syntax::ParseResult {
    static PARSED: OnceLock<vela_syntax::ParseResult> = OnceLock::new();
    PARSED.get_or_init(|| {
        let parsed = parse(vela_span::FileId::from_raw(0), SOURCE);
        debug_assert!(
            parsed.diagnostics.is_empty(),
            "the interface must parse: {:?}",
            parsed.diagnostics
        );
        parsed
    })
}

/// The interface's screens, in declaration order.
#[must_use]
pub fn decls() -> &'static [&'static ScreenDecl] {
    static DECLS: OnceLock<Vec<&'static ScreenDecl>> = OnceLock::new();
    DECLS.get_or_init(|| screens(parsed()))
}

/// A screen the interface declares, by name.
#[must_use]
pub fn decl(name: &str) -> Option<&'static ScreenDecl> {
    decls().iter().copied().find(|screen| screen.name == name)
}

/// The interface's screens as a set, with its own theme.
///
/// A set of its own, because that is what a set is: one file's screens, styles, and theme. A caller
/// that lays an interface screen by name adds this to the sets it searches, which is how
/// `open_screen("game_menu", …)` finds a screen no project file declares.
#[must_use]
pub fn set() -> ScreenSet {
    ScreenSet::from_items(&parsed().program.items)
}

/// Every screen a parsed file declares, in declaration order.
fn screens(parsed: &'static vela_syntax::ParseResult) -> Vec<&'static ScreenDecl> {
    parsed
        .program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Screen(screen) => Some(screen),
            _ => None,
        })
        .collect()
}
