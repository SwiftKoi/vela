use vela_diag::Diagnostic;
use vela_span::Span;
use vela_syntax::{Expr, ScreenArg, ScreenLine, StrPart};

use crate::actions::{ActionRegistry, SET_SCREEN_VARIABLE};
use crate::eval::is_question_call;

use super::diag;
use super::screen::Scope;
use super::settings::check_setting_call;
use super::variants::check_variant;

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
                    check_actions(arm, actions, scope, out);
                }
            }
            // A loop's body is drawn, so its actions are checked like any other. Its *iterable* is a
            // value rather than an action position: `for i in range(6)` produces a sequence, and the
            // element of a list may still be an action (`for a in [quit()]`), which is what
            // `sequence` looks inside for. Reading the iterable as an action position reported a
            // producer as a misspelled action — `E5012: no action called `range`` — which is the same
            // misreading the condition walk had, one line further down (`SCREENS.md §2.4`).
            ScreenLine::For { iterable, body, .. } => {
                sequence(iterable, actions, scope, out);
                check_actions(body, actions, scope, out);
            }
            // A binding's action is an action like any other, so a misspelled one is the same
            // mistake here as in an `action` prop — and the delay is an expression that may hold
            // one too.
            ScreenLine::Key { action, .. } => check_action_expr(action, actions, scope, out),
            ScreenLine::Timer {
                seconds, action, ..
            } => {
                check_action_expr(seconds, actions, scope, out);
                check_action_expr(action, actions, scope, out);
            }
            // A variable's initializer is an expression like any other, and one may name an action —
            // `default choice = close_screen()` is a screen whose first state is an action.
            ScreenLine::Default { value, .. } => check_action_expr(value, actions, scope, out),
            ScreenLine::Use { args, body, .. } => {
                for arg in args {
                    if let Some(value) = arg_value(arg) {
                        check_action_expr(value, actions, scope, out);
                    }
                }
                check_actions(body, actions, scope, out);
            }
            ScreenLine::Node(node) => {
                for arg in &node.args {
                    if let Some(value) = arg_value(arg) {
                        check_action_expr(value, actions, scope, out);
                    }
                }
                check_actions(&node.children, actions, scope, out);
            }
        }
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

/// The actions an iterable holds: the ones written *inside* it, not the call that produces it.
///
/// The head of an iterable is a producer — a name, a call, a literal — and a call there is a value
/// the screen draws from rather than an action it performs. What its *elements* are is a different
/// question, and a list literal is the one shape where an element can be an action.
fn sequence(expr: &Expr, actions: &ActionRegistry, scope: Scope<'_>, out: &mut Vec<Diagnostic>) {
    match expr {
        Expr::List { items, .. } => {
            for item in items {
                check_action_expr(item, actions, scope, out);
            }
        }
        Expr::Paren { inner, .. } => sequence(inner, actions, scope, out),
        Expr::Binary { lhs, rhs, .. } => {
            sequence(lhs, actions, scope, out);
            sequence(rhs, actions, scope, out);
        }
        Expr::If { then_, else_, .. } => {
            sequence(then_, actions, scope, out);
            sequence(else_, actions, scope, out);
        }
        _ => {}
    }
}

/// Checks one expression, and every expression inside it.
fn check_action_expr(
    expr: &Expr,
    actions: &ActionRegistry,
    scope: Scope<'_>,
    out: &mut Vec<Diagnostic>,
) {
    match expr {
        Expr::Call { callee, args, span } => {
            // `foo.bar()` is a call on a value the screen holds, not a registry name: the language
            // has no such action, so there is nothing here to check.
            if let Expr::Name { name, .. } = callee.as_ref() {
                if is_question_call(callee) {
                    // A question the host answers is not an action (`SCREENS.md §2.6`): `variant("pc")`
                    // is a value, and the registry has nothing to say about it. What has something to
                    // say is the *vocabulary of names*, which `check/variants.rs` asks about.
                    check_variant(args, *span, out);
                } else {
                    check_action(name, args, *span, actions, scope, out);
                    // The two calls that *name a setting* are held to that vocabulary too, and here
                    // rather than in `check_action`: the registry knows the action's name and its
                    // arity, and `check/settings.rs` knows which settings exist.
                    check_setting_call(name, args, *span, scope, out);
                }
            }
            check_action_expr(callee, actions, scope, out);
            for arg in args {
                check_action_expr(arg, actions, scope, out);
            }
        }
        Expr::Str { parts, .. } => {
            for part in parts {
                if let vela_syntax::StrPart::Interpolation { expr, .. } = part {
                    check_action_expr(expr, actions, scope, out);
                }
            }
        }
        Expr::Paren { inner, .. } | Expr::Field { base: inner, .. } => {
            check_action_expr(inner, actions, scope, out);
        }
        Expr::Unary { operand, .. } => check_action_expr(operand, actions, scope, out),
        Expr::Binary { lhs, rhs, .. } => {
            check_action_expr(lhs, actions, scope, out);
            check_action_expr(rhs, actions, scope, out);
        }
        Expr::Index { base, index, .. } => {
            check_action_expr(base, actions, scope, out);
            check_action_expr(index, actions, scope, out);
        }
        Expr::List { items, .. } => {
            for item in items {
                check_action_expr(item, actions, scope, out);
            }
        }
        Expr::Map { entries, .. } => {
            for (key, value) in entries {
                check_action_expr(key, actions, scope, out);
                check_action_expr(value, actions, scope, out);
            }
        }
        Expr::If {
            cond, then_, else_, ..
        } => {
            check_action_expr(cond, actions, scope, out);
            check_action_expr(then_, actions, scope, out);
            check_action_expr(else_, actions, scope, out);
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

    if action.arity() != args.len() {
        out.push(
            diag(
                "E5013",
                format!(
                    "`{}` takes {} argument(s), but was given {}",
                    action.name,
                    action.arity(),
                    args.len()
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
