//! What the stack does about an action a screen asked for.
//!
//! A screen asks once and is answered three ways: the focused control, a `key` binding, and — since a
//! test that clicks is a player minus the window (`TOOLING.md §5`) — a test. One dispatcher rather
//! than three copies, because three copies are three places for `hide` to come to mean different
//! things (`SCREENS.md §7`).
//!
//! Only what a *stack* can do is here: open a screen, close one, hide one, write a screen's own
//! variable. Everything else — `quit`, `jump`, a save, the rollback history — is the VM's or the
//! host's, and comes back as [`Done::NotOurs`] for the caller that has one. That split is the point:
//! a player runs what it gets back, and a headless test says that it cannot.

use vela_text::TextEngine;

use crate::Value;
use crate::actions::Action;

use super::{ScreenSource, Stack};

/// What the stack did about an action a screen asked for.
///
/// An outcome rather than a line of output, because the callers say different things about the same
/// event: a window writes what happened where a harness reading a pipe can see it
/// (`commands/play.rs`), and a headless run reports a failure from it (`vela-test`). The *decision* —
/// which actions the stack carries out, and what each one does — is the shared part, and this is it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Done {
    /// `open_screen`: the screen was laid out and pushed.
    Opened(String),
    /// `open_screen`: no screen by that name is declared.
    Missing(String),
    /// `close_screen`: the top screen was popped.
    Closed(String),
    /// `hide`: the named screen was removed from the stack.
    Hidden(String),
    /// `set_screen_variable`: the top screen's variable was written, and it was laid out again.
    Set(String),
    /// `set_screen_variable`: the screen it names is no longer open.
    Stale(String),
    /// The action had nothing to act on: no argument, or nothing open.
    Nothing,
    /// Not the stack's to carry out. The caller decides what it means — a player runs it, a test
    /// reports that a headless run does not.
    NotOurs,
}

impl Stack {
    /// Carries out the part of an action this stack owns, and says what it did.
    ///
    /// One place, because a screen asks three ways now — the focused control, a `key` binding, and a
    /// test that clicks — and a second copy of this match would be a second place for them to
    /// disagree about what `hide` means.
    pub fn dispatch(
        &mut self,
        action: &Action,
        screens: &(impl ScreenSource + ?Sized),
        size: (u32, u32),
        text: &mut TextEngine,
        font: &str,
    ) -> Done {
        match action.name.as_str() {
            "open_screen" => {
                let Some(name) = action.first() else {
                    return Done::Nothing;
                };
                match self.open(screens, name, size, text, font) {
                    true => Done::Opened(name.to_string()),
                    false => Done::Missing(name.to_string()),
                }
            }
            "close_screen" => match self.close() {
                Some(name) => Done::Closed(name),
                None => Done::Nothing,
            },
            "hide" => {
                let Some(name) = action.first() else {
                    return Done::Nothing;
                };
                match self.close_named(name) {
                    true => Done::Hidden(name.to_string()),
                    false => Done::Nothing,
                }
            }
            // The one action whose argument is a *value* rather than a name (`SCREENS.md §2.5`): the
            // screen resolved it, so what arrives is what the variable becomes — the string, or the
            // number, rather than the words it was written with.
            "set_screen_variable" => {
                let (Some(Value::Str(name)), Some(value)) =
                    (action.args.first(), action.args.get(1))
                else {
                    return Done::Nothing;
                };
                let (name, value) = (name.clone(), value.clone());
                match self.set_variable(screens, &name, value, size, text, font) {
                    true => Done::Set(name),
                    false => Done::Stale(name),
                }
            }
            // The rest need the VM or the `World`, and `SCREENS.md §7` says which: the registry
            // carries the same answer, and a caller that has neither says so rather than doing
            // nothing quietly.
            _ => Done::NotOurs,
        }
    }
}
