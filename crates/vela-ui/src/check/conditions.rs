//! `W4013` — a screen condition that calls something, and `E5018` for a question's name.

use vela_diag::Diagnostic;
use vela_syntax::{Expr, ScreenLine, StrPart};

use crate::eval::is_question_call;

use super::diag;
use super::variants::check_variant;
use super::walk::nested;

/// Reports every call written in a condition, at every nesting.
///
/// A screen decides from what it *has*: its parameters, its variables (`SCREENS.md §2.5`), what a loop
/// bound, the literals and comparisons over them (`§2.2`), and the questions the host answers (`§2.6`).
/// A call that is not one of those reads like a decision and is not one — `if GamepadExists():` is
/// false, and the screen draws the wrong arm without saying anything. `renpy.variant("small")` was the
/// same shape until it became a question, so this warning is also what says a screen *needs* §7's host
/// systems rather than that the screen is broken.
pub(super) fn check_conditions(lines: &[ScreenLine], out: &mut Vec<Diagnostic>) {
    for line in lines {
        if let ScreenLine::If {
            condition, elifs, ..
        } = line
        {
            check_calls(condition, out);
            for clause in elifs {
                check_calls(&clause.condition, out);
            }
        }
        for body in nested(line) {
            check_conditions(body, out);
        }
    }
}

/// Reports the calls in one condition, one diagnostic each.
///
/// One each rather than one for the condition, because `A() or B()` has two things that cannot be
/// decided and a reader who fixes only the first would still have a condition that is always false.
/// A *question* is the other case: it can be decided, so the only thing to report is its name
/// (`E5018`) — which the action walk cannot do for a condition, because a condition is not an action
/// position and that walk no longer reaches one.
fn check_calls(expr: &Expr, out: &mut Vec<Diagnostic>) {
    match expr {
        Expr::Call { callee, args, span } => {
            if is_question_call(callee) {
                check_variant(args, *span, out);
            } else {
                // `foo.bar()` is a call on a value rather than a name a registry knows; it cannot be
                // decided either, and naming it after the field is what a reader can act on.
                let name = match callee.as_ref() {
                    Expr::Name { name, .. } => name.as_str(),
                    Expr::Field { name, .. } => name.as_str(),
                    _ => "",
                };
                out.push(
                    diag(
                        "W4013",
                        format!("`{name}()` cannot decide a condition"),
                        *span,
                        "a screen decides from what it has, and a call is not one of those",
                    )
                    .with_help(
                        "a screen reads a parameter, a variable, a loop element, a literal, or a \
                         question the host answers (`variant`)",
                    ),
                );
            }
            check_calls(callee, out);
            for arg in args {
                check_calls(arg, out);
            }
        }
        Expr::Paren { inner, .. } => check_calls(inner, out),
        Expr::Unary { operand, .. } => check_calls(operand, out),
        Expr::Binary { lhs, rhs, .. } => {
            check_calls(lhs, out);
            check_calls(rhs, out);
        }
        Expr::Field { base, .. } => check_calls(base, out),
        Expr::Index { base, index, .. } => {
            check_calls(base, out);
            check_calls(index, out);
        }
        Expr::List { items, .. } => {
            for item in items {
                check_calls(item, out);
            }
        }
        Expr::Map { entries, .. } => {
            for (key, value) in entries {
                check_calls(key, out);
                check_calls(value, out);
            }
        }
        Expr::Str { parts, .. } => {
            for part in parts {
                if let StrPart::Interpolation { expr, .. } = part {
                    check_calls(expr, out);
                }
            }
        }
        // A literal, a name, or the error a recovery already reported: no call to find, and a lambda's
        // body runs when something calls it — which no screen does.
        _ => {}
    }
}
