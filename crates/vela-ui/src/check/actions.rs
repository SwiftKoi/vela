use vela_diag::Diagnostic;
use vela_span::Span;
use vela_syntax::{Expr, ScreenArg, ScreenLine, StrPart};

use crate::actions::{ActionRegistry, SET_SCREEN_VARIABLE};

use super::diag;
use super::screen::Scope;
use super::settings::{check_question, check_setting_call};

/// Checks every action a body calls against the registry.
///
/// A *walk over expressions* rather than a check of the `action` prop, because that prop is not the
/// only place an action is written: `use confirm("Stop?", quit(), close_screen())` passes two of them
/// as arguments, and a misspelled one there is the same mistake. The registry was a docs source
/// before this — nothing consulted it, so `action quitt()` was accepted and did nothing at all.
pub(super) fn check_actions(
    lines: &[ScreenLine],
    actions: &ActionRegistry,
    scope: Scope<'_>,
    out: &mut Vec<Diagnostic>,
) {
    each_call(lines, &mut |name, args, span| {
        if crate::eval::is_question(name) {
            // A question the host answers is not an action (`SCREENS.md §2.6`): `variant("pc")` is a
            // value, and the registry has nothing to say about it. What has something to say is the
            // *vocabulary of names*, which `check/settings.rs` dispatches.
            check_question(name, args, span, scope, out);
            return;
        }
        check_action(name, args, span, actions, scope, out);
        // The two calls that *name a setting* are held to that vocabulary too, and here rather than in
        // `check_action`: the registry knows the action's name and its arity, and `check/settings.rs`
        // knows which settings exist.
        check_setting_call(name, args, span, scope, out);
    });
}

/// Every action call a body writes, and where it is written.
///
/// One traversal, because "where an action can be written" is one list with several entries
/// (`SCREENS.md §7`) — an `action` prop, a `key` binding, a `timer`'s action, a `default`'s
/// initializer, an argument of a widget or of a `use` — and a second walk for a second check would be a
/// second place for that list to fall out of date.
///
/// The *call's name* is what is handed over, and its arguments as written: a check that wants the
/// registry looks the name up, and one that wants a vocabulary of its own (`check/settings.rs`,
/// `check/opens.rs`) reads the arguments. Conditions are not walked — a call in one is a condition that
/// cannot be decided rather than an action, and `conditions.rs` reports it (`W4013`) — and neither is a
/// loop's iterable, which produces values rather than performing anything.
pub(super) fn each_call<'a>(
    lines: &'a [ScreenLine],
    visit: &mut impl FnMut(&'a str, &'a [Expr], Span),
) {
    for line in lines {
        match line {
            ScreenLine::Layer { .. }
            | ScreenLine::StylePrefix { .. }
            | ScreenLine::Transclude { .. } => {}
            // The *bodies* of an `if`, and not its conditions: a condition is decided by truthiness
            // rather than read as a value, so a call in one is not an action — it is a condition that
            // cannot be decided, and `conditions.rs` is what reports it (`W4013`). Reporting both would
            // be two diagnostics for one mistake, and `E5012` would be the wrong one.
            ScreenLine::If { .. } => {
                for arm in line.bodies() {
                    each_call(arm, visit);
                }
            }
            // A loop's body is drawn, so its actions are checked like any other. Its *iterable* is a
            // value rather than an action position: `for i in range(6)` produces a sequence, and the
            // element of a list may still be an action (`for a in [quit()]`), which is what
            // `sequence` looks inside for. Reading the iterable as an action position reported a
            // producer as a misspelled action — `E5012: no action called `range`` — which is the same
            // misreading the condition walk had, one line further down (`SCREENS.md §2.4`).
            ScreenLine::For { iterable, body, .. } => {
                each_in(iterable, visit);
                each_call(body, visit);
            }
            // A binding's action is an action like any other, so a misspelled one is the same
            // mistake here as in an `action` prop — and the delay is an expression that may hold
            // one too.
            ScreenLine::Key { action, .. } => each_expr(action, visit),
            ScreenLine::Timer {
                seconds, action, ..
            } => {
                each_expr(seconds, visit);
                each_expr(action, visit);
            }
            // A variable's initializer is an expression like any other, and one may name an action —
            // `default choice = close_screen()` is a screen whose first state is an action.
            ScreenLine::Default { value, .. } => each_expr(value, visit),
            ScreenLine::Use { args, body, .. } => {
                for arg in args {
                    if let Some(value) = arg_value(arg) {
                        each_expr(value, visit);
                    }
                }
                each_call(body, visit);
            }
            ScreenLine::Node(node) => {
                for arg in &node.args {
                    if let Some(value) = arg_value(arg) {
                        each_expr(value, visit);
                    }
                }
                each_call(&node.children, visit);
            }
        }
    }
}

/// The actions an iterable holds: the ones written *inside* it, not the call that produces it.
fn each_in<'a>(expr: &'a Expr, visit: &mut impl FnMut(&'a str, &'a [Expr], Span)) {
    match expr {
        Expr::List { items, .. } => {
            for item in items {
                each_expr(item, visit);
            }
        }
        Expr::Paren { inner, .. } => each_in(inner, visit),
        Expr::Binary { lhs, rhs, .. } => {
            each_in(lhs, visit);
            each_in(rhs, visit);
        }
        Expr::If { then_, else_, .. } => {
            each_in(then_, visit);
            each_in(else_, visit);
        }
        _ => {}
    }
}

/// Every action call inside one expression.
fn each_expr<'a>(expr: &'a Expr, visit: &mut impl FnMut(&'a str, &'a [Expr], Span)) {
    match expr {
        Expr::Call { callee, args, span } => {
            // `foo.bar()` is a call on a value the screen holds, not a registry name: the language
            // has no such action, so there is nothing here to check.
            if let Expr::Name { name, .. } = callee.as_ref() {
                visit(name, args, *span);
            }
            each_expr(callee, visit);
            for arg in args {
                each_expr(arg, visit);
            }
        }
        Expr::Str { parts, .. } => {
            for part in parts {
                if let vela_syntax::StrPart::Interpolation { expr, .. } = part {
                    each_expr(expr, visit);
                }
            }
        }
        Expr::Paren { inner, .. } | Expr::Field { base: inner, .. } => each_expr(inner, visit),
        Expr::Unary { operand, .. } => each_expr(operand, visit),
        Expr::Binary { lhs, rhs, .. } => {
            each_expr(lhs, visit);
            each_expr(rhs, visit);
        }
        Expr::Index { base, index, .. } => {
            each_expr(base, visit);
            each_expr(index, visit);
        }
        Expr::List { items, .. } => {
            for item in items {
                each_expr(item, visit);
            }
        }
        Expr::Map { entries, .. } => {
            for (key, value) in entries {
                each_expr(key, visit);
                each_expr(value, visit);
            }
        }
        Expr::If {
            cond, then_, else_, ..
        } => {
            each_expr(cond, visit);
            each_expr(then_, visit);
            each_expr(else_, visit);
        }
        Expr::Int { .. }
        | Expr::Float { .. }
        | Expr::Bool { .. }
        | Expr::None { .. }
        | Expr::Path { .. }
        | Expr::Name { .. }
        // A lambda's body runs when something calls it, which no screen does; an action cannot be
        // written in one.
        | Expr::Lambda { .. }
        | Expr::Error { .. } => {}
    }
}

/// The expression an argument carries, if it carries one.
fn arg_value(arg: &ScreenArg) -> Option<&Expr> {
    match arg {
        ScreenArg::Value(value) => Some(value),
        ScreenArg::Named {
            value: Some(value), ..
        } => Some(value),
        // A bare name is a flag (`stretch_x`) or a leaf's content (`text line`) — not a value, and
        // so not an action.
        ScreenArg::Named { value: None, .. } => None,
    }
}

/// One action call: the name is registered, and it takes this many arguments.
fn check_action(
    name: &str,
    args: &[Expr],
    span: Span,
    actions: &ActionRegistry,
    scope: Scope<'_>,
    out: &mut Vec<Diagnostic>,
) {
    let Some(action) = actions.get(name) else {
        let mut diagnostic = diag(
            "E5012",
            format!("no action called `{name}`"),
            span,
            "no action by this name is registered",
        );
        if let Some(nearest) = actions.closest(name) {
            diagnostic = diagnostic.with_help(format!("did you mean `{nearest}`?"));
        }
        out.push(diagnostic);
        return;
    };

    // A call must give the arguments the action declares, and no more *unless* the entry declares a
    // rest (`open_screen`'s belong to the screen it opens) — so this is a floor plus a ceiling that
    // some entries do not have.
    let given = args.len();
    if given < action.arity() || (given > action.arity() && !action.takes_rest()) {
        out.push(
            diag(
                "E5013",
                format!(
                    "`{}` takes {} argument(s), but was given {}",
                    action.name,
                    action.arity(),
                    given
                ),
                span,
                "the arguments do not match the action",
            )
            .with_help(format!("write it as `{}`", action.signature())),
        );
    }

    if name == SET_SCREEN_VARIABLE {
        check_write(args.first(), scope.variables, out);
    }
}

/// `E5017` — a screen writes a variable it does not declare (`SCREENS.md §2.5`).
///
/// The store `set_screen` writes belongs to one screen *instance*, so the name must be something this
/// screen declared: not a world variable, which a screen never mutates, and not a parameter, which is
/// the caller's value. Checked because the alternative is a button that writes a name nothing reads —
/// and because the name is written as a name, which is the one thing Ren'Py's stringly-typed form
/// cannot be held to.
fn check_write(first: Option<&Expr>, declared: &[&str], out: &mut Vec<Diagnostic>) {
    let Some((variable, span)) = first.and_then(written_name) else {
        return;
    };
    if declared.contains(&variable) {
        return;
    }
    let mut diagnostic = diag(
        "E5017",
        format!("`{variable}` is not a variable of this screen"),
        span,
        "this screen declares no variable by that name",
    );
    if let Some(nearest) = vela_diag::closest(variable, declared.iter().copied()) {
        diagnostic = diagnostic.with_help(format!("did you mean `{nearest}`?"));
    }
    out.push(diagnostic);
}

/// The name an argument writes, and where it is written, when it is written as a name.
///
/// A bare name is the variable's name — not a lookup, because the store it writes is named statically —
/// and a single-literal string is the same thing, since a name is all it holds. An interpolated string
/// or an expression is not a name and is not checked, which is the one case this cannot see.
fn written_name(expr: &Expr) -> Option<(&str, Span)> {
    match expr {
        Expr::Name { name, span } => Some((name, *span)),
        Expr::Str { parts, span } => match parts.as_slice() {
            [StrPart::Literal { text, .. }] => Some((text, *span)),
            _ => None,
        },
        _ => None,
    }
}
