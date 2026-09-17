//! `W4013` — a screen condition that calls something.

use vela_diag::Diagnostic;
use vela_span::Span;
use vela_syntax::{Expr, ScreenLine, StrPart};

use super::diag;
use super::walk::nested;

/// Reports every call written in a condition, at every nesting.
///
/// A screen decides from what it *has*: its parameters, its variables (`SCREENS.md §2.5`), what a loop
/// bound, and the literals and comparisons over them (`§2.2`). A call is the one shape left that reads
/// like a decision and is not one — `if GamepadExists():` is false, and the screen draws the wrong arm
/// without saying anything. `renpy.variant("small")` is the same shape, so this is also the warning that
/// says a screen *needs* §7's host systems rather than that the screen is broken.
pub(super) fn check_conditions(lines: &[ScreenLine], out: &mut Vec<Diagnostic>) {
    for line in lines {
        if let ScreenLine::If {
            condition, elifs, ..
        } = line
        {
            report(condition, out);
            for clause in elifs {
                report(&clause.condition, out);
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
fn report(condition: &Expr, out: &mut Vec<Diagnostic>) {
    let mut calls = Vec::new();
    calls_in(condition, &mut calls);
    for (name, span) in calls {
        out.push(
            diag(
                "W4013",
                format!("`{name}()` cannot decide a condition"),
                span,
                "a screen decides from what it has, and a call is not one of those",
            )
            .with_help("a screen reads a parameter, a variable, a loop element, or a literal"),
        );
    }
}

/// Every call an expression contains, in the order they are written.
fn calls_in<'a>(expr: &'a Expr, out: &mut Vec<(&'a str, Span)>) {
    match expr {
        Expr::Call { callee, args, span } => {
            // `foo.bar()` is a call on a value rather than a name a registry knows; it cannot be decided
            // either, and naming it after the field is what a reader can act on.
            let name = match callee.as_ref() {
                Expr::Name { name, .. } => name.as_str(),
                Expr::Field { name, .. } => name.as_str(),
                _ => "",
            };
            out.push((name, *span));
            calls_in(callee, out);
            for arg in args {
                calls_in(arg, out);
            }
        }
        Expr::Paren { inner, .. } => calls_in(inner, out),
        Expr::Unary { operand, .. } => calls_in(operand, out),
        Expr::Binary { lhs, rhs, .. } => {
            calls_in(lhs, out);
            calls_in(rhs, out);
        }
        Expr::Field { base, .. } => calls_in(base, out),
        Expr::Index { base, index, .. } => {
            calls_in(base, out);
            calls_in(index, out);
        }
        Expr::List { items, .. } => {
            for item in items {
                calls_in(item, out);
            }
        }
        Expr::Map { entries, .. } => {
            for (key, value) in entries {
                calls_in(key, out);
                calls_in(value, out);
            }
        }
        Expr::Str { parts, .. } => {
            for part in parts {
                if let StrPart::Interpolation { expr, .. } = part {
                    calls_in(expr, out);
                }
            }
        }
        // A literal, a name, or the error a recovery already reported: no call to find, and a lambda's
        // body runs when something calls it — which no screen does.
        _ => {}
    }
}
