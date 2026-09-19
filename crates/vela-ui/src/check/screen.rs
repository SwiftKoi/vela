//! The checker's entry point: every rule a screen is held to, asked once.
//!
//! Apart from the four walks because a `mod.rs` declares modules and re-exports, and because the
//! question this file answers is a different one from any of theirs: *"is this screen legal"* rather
//! than "is this line a widget" or "is this call an action".

use vela_diag::{Code, Diagnostic};
use vela_span::Span;
use vela_syntax::{Param, ScreenDecl, ScreenLine};

use crate::actions::ActionRegistry;
use crate::compose;
use crate::input::SemanticActions;
use crate::widgets::WidgetRegistry;

use super::actions::check_actions;
use super::conditions::check_conditions;
use super::keys::check_keys;
use super::variables::check_declares;
use super::widgets::check_lines;

/// Builds a diagnostic for a registered code.
///
/// # Panics
///
/// Panics if the code is not registered, which is a bug here rather than anything a user can
/// trigger: `check-diag-codes` rejects an unregistered code before it can be committed.
pub(crate) fn diag(
    code: &str,
    message: impl Into<String>,
    span: Span,
    label: impl Into<String>,
) -> Diagnostic {
    let code =
        Code::new(code).unwrap_or_else(|| panic!("`{code}` is not in crates/vela-diag/codes.txt"));
    Diagnostic::new(code, message, span, label)
}

/// The names a screen has: what it declared, and what it was given.
///
/// Two lists rather than one, because two rules ask different questions of them. A *setting's* name
/// must not be a name the screen has — `preference(which, 30)` is a computed name, and a computed name
/// is not a setting anybody can look up — so that check asks whether the screen has the name at all.
/// `set_screen_variable` must name a *variable* the screen declared: a parameter is not writable,
/// because the write would land in the screen's own store while the parameter kept reading what it was
/// given (`E5017`).
#[derive(Clone, Copy)]
pub(super) struct Scope<'a> {
    /// The screen's own `default` variables.
    pub(super) variables: &'a [&'a str],
    /// What the screen was given.
    pub(super) params: &'a [&'a str],
}

impl Scope<'_> {
    /// Whether the screen has this name at all, as a variable or as a parameter.
    pub(super) fn has(&self, name: &str) -> bool {
        self.variables.contains(&name) || self.params.contains(&name)
    }
}

/// Checks a screen body against the registry and the file's other screens.
///
/// `screens` is the file's declarations rather than a lookup built here, because composition is
/// per-file (`SCREENS.md §5`) and a caller that already has the parsed items should not have to build
/// a second index for the same question.
///
/// `params` is what the screen was declared to take, so that a variable cannot be named after one of
/// them (`E5016`): the caller's answer and the screen's own would be two answers to one question.
#[must_use]
pub fn check_screen(
    lines: &[ScreenLine],
    params: &[Param],
    registry: &WidgetRegistry,
    screens: &[&ScreenDecl],
    actions: &ActionRegistry,
    inputs: &SemanticActions,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    // A screen's top level has no parent widget, so a bare prop there has nothing to belong
    // to and a line must be a widget.
    check_lines(lines, None, registry, &mut diagnostics);
    // `use` and `transclude` are a relationship between declarations, so their rules live together
    // in one place rather than being spread through this walk (`compose.rs`). The graph *between*
    // screens is a per-file question, so it is `compose::check_cycles`, asked once.
    compose::check_uses(screens, lines, &mut diagnostics);
    // The screen's own variables: where they may be declared, and whether `set_screen_variable` can
    // write one. Both are rules about the screen rather than about a line, so they are asked here
    // (`§2.5`).
    check_declares(lines, params, &mut diagnostics);
    let variables: Vec<&str> = ScreenLine::declares(lines)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    let given: Vec<&str> = params.iter().map(|param| param.name.as_str()).collect();
    let scope = Scope {
        variables: &variables,
        params: &given,
    };
    // Every call in a body is an action (`SCREENS.md §7`), and the registry is what says so.
    check_actions(lines, actions, scope, &mut diagnostics);
    // Every `key` names a semantic action (`§11`), and the vocabulary is what says so.
    check_keys(lines, inputs, &mut diagnostics);
    // And a condition has to be decidable: a screen decides from what it has, not from a call (`§2.2`).
    check_conditions(lines, scope, &mut diagnostics);
    diagnostics
}
