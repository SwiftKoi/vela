//! `E5020` and `E5021` — a setting a screen writes that the vocabulary does not have.

use vela_diag::Diagnostic;
use vela_span::Span;
use vela_syntax::{Expr, StrPart};

use crate::actions::{PREFERENCE, TOGGLE_PREFERENCE};
use crate::settings::{SettingDecl, SettingTy};

use super::diag;
use super::screen::Scope;

/// Checks the setting a `preference(...)` or `toggle_preference(...)` call names (`SCREENS.md §7`).
///
/// `flip` is the difference between the two calls: `toggle_preference(name)` flips a boolean, and
/// `preference(name, value)` writes a value. Both name a setting, and both are held to the same table
/// — what that buys is the point of the check: Ren'Py's `Preference("text speed")` is a *string*
/// looked up at run time, so a misspelling becomes a control that silently does nothing, which is
/// exactly what the sample's migrated settings screen was doing
/// (`docs/roadmap/M12.2-game-interface.md`).
///
/// `scope` is the screen's own names — its parameters and variables — because a bare name means two
/// things and the scope is what tells them apart: a *word* the screen writes (`text_speed`) when the
/// screen has no such name, and an expression when it has (`preference(which, 30)` is a computed name,
/// which cannot be looked up and is not a setting).
pub(super) fn check_setting_call(
    call: &str,
    args: &[Expr],
    span: Span,
    scope: Scope<'_>,
    out: &mut Vec<Diagnostic>,
) {
    match call {
        PREFERENCE => check_setting(false, args, span, scope, out),
        TOGGLE_PREFERENCE => check_setting(true, args, span, scope, out),
        _ => {}
    }
}

/// One call's setting, and the value it writes when it writes one.
fn check_setting(
    flip: bool,
    args: &[Expr],
    span: Span,
    scope: Scope<'_>,
    out: &mut Vec<Diagnostic>,
) {
    let Some(setting) = named(args.first(), span, scope, out) else {
        return;
    };
    if flip {
        if setting.ty != SettingTy::Bool {
            out.push(
                diag(
                    "E5021",
                    format!(
                        "`{}` is not `true` or `false`, so there is nothing to flip",
                        setting.name
                    ),
                    span,
                    format!("it takes {}", setting.ty.describe()),
                )
                .with_help("`toggle_preference` flips a boolean; `preference` writes any setting"),
            );
        }
        return;
    }
    // The value. A literal is checked here; an expression is the runtime's to answer, and
    // `SCREENS.md §7` says so rather than implying this check is total — which is also why a choice is
    // written as a string: `preference(display_mode, "fullscreen")` is checkable, where
    // `preference(display_mode, fullscreen)` is a *name*, and where a name points is the screen-name
    // check's question (`E2001`).
    let Some(written) = args.get(1).and_then(literal) else {
        return;
    };
    if setting.ty.takes(&written) {
        return;
    }
    out.push(
        diag(
            "E5021",
            format!("`{}` does not take `{written}`", setting.name),
            span,
            format!("it takes {}", setting.ty.describe()),
        )
        .with_help(format!(
            "the settings a screen can write: {}",
            SettingDecl::names().join(", ")
        )),
    );
}

/// The setting a call names, or `None` — with the entry — when it does not name one.
fn named(
    arg: Option<&Expr>,
    span: Span,
    scope: Scope<'_>,
    out: &mut Vec<Diagnostic>,
) -> Option<&'static SettingDecl> {
    let known = SettingDecl::names();
    let Some(expr) = arg else {
        // No argument at all: the arity is the registry's question, and `E5013` is what answers it.
        return None;
    };
    // A name the screen declares is an expression rather than a setting, and a computed setting is the
    // one case worth saying out loud: it cannot be checked at all.
    if is_declared(expr, scope) {
        out.push(
            diag(
                "E5020",
                "a setting's name has to be written out".to_string(),
                span,
                "a setting is checked where it is written",
            )
            .with_help(format!("write one of: {}", known.join(", "))),
        );
        return None;
    }
    let written = word(expr, scope)?;
    if let Some(setting) = SettingDecl::named(&written) {
        return Some(setting);
    }
    let entry = diag(
        "E5020",
        format!("`{written}` is not a setting"),
        span,
        format!("this build knows {}", known.join(", ")),
    );
    out.push(match SettingDecl::closest(&written) {
        Some(suggestion) => entry.with_help(format!("did you mean `{suggestion}`?")),
        None => entry,
    });
    None
}

/// The word an argument writes, when it writes one rather than computing one.
///
/// A setting's name is **written as a string** — `preference("text_speed", 30)`, which is what the
/// migration emits and what the reference shows — because a bare word is an *expression*, and a name
/// nothing declares is `E2001`'s to report rather than this check's: two diagnostics for one mistake is
/// the thing the checker avoids (`check/actions.rs` says the same about `W4013` and `E5012`).
///
/// The exception is a name the *screen* declares, and it is here for the same reason in reverse: a
/// computed setting cannot be checked, and nothing else would say so — the name is in scope, so `E2001`
/// is silent about it.
fn word(expr: &Expr, scope: Scope<'_>) -> Option<String> {
    match expr {
        Expr::Str { parts, .. } => match parts.as_slice() {
            [StrPart::Literal { text, .. }] => Some(text.clone()),
            _ => None,
        },
        Expr::Name { name, .. } if scope.has(name) => Some(name.clone()),
        Expr::Paren { inner, .. } => word(inner, scope),
        _ => None,
    }
}

/// Whether an argument is a name this screen has.
fn is_declared(expr: &Expr, scope: Scope<'_>) -> bool {
    match expr {
        Expr::Name { name, .. } => scope.has(name),
        Expr::Paren { inner, .. } => is_declared(inner, scope),
        _ => false,
    }
}

/// A *value* the checker can see whole: a literal, and not a bare name.
///
/// The difference from [`word`] is deliberate. `preference(text_speed, other_speed)` — where the name
/// is a global — is an expression, and a checker that read it as the word `other_speed` would report a
/// correct screen. So a value is checked only when it is a literal, and a word is written as a string.
fn literal(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Str { parts, .. } => match parts.as_slice() {
            [StrPart::Literal { text, .. }] => Some(text.clone()),
            _ => None,
        },
        Expr::Int { value, .. } => Some(value.to_string()),
        Expr::Float { value, .. } => Some(vela_world::format_float(*value)),
        Expr::Bool { value, .. } => Some(value.to_string()),
        Expr::Paren { inner, .. } => literal(inner),
        _ => None,
    }
}
