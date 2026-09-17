use vela_diag::Diagnostic;
use vela_syntax::{Param, ScreenLine};

use super::diag;
use super::walk::nested;

/// `E5015`, `E5016` — where a screen variable may be declared, and how often.
///
/// A `default` is neither a widget nor a prop, so it is not the widget walk's business: both of these
/// rules are about the *screen* the line sits in, and reporting them from one place is what keeps the
/// instantiator from having to decide what a variable declared inside an arm would even mean.
///
/// (`E5017`, the write, is in `actions.rs`: a write is a call, and finding the calls is that walk's job.)
pub(super) fn check_declares(lines: &[ScreenLine], params: &[Param], out: &mut Vec<Diagnostic>) {
    let mut seen: Vec<&str> = Vec::new();
    for line in lines {
        let ScreenLine::Default { span, name, .. } = line else {
            continue;
        };
        // A parameter and a variable of the same name are two answers to one question, and the caller's
        // answer is the one the screen would silently ignore.
        if params.iter().any(|param| param.name == *name) {
            out.push(
                diag(
                    "E5016",
                    format!("`{name}` is already a parameter of this screen"),
                    *span,
                    "a screen variable cannot have the name the caller passes",
                )
                .with_help("rename one of the two"),
            );
            continue;
        }
        if seen.contains(&name.as_str()) {
            out.push(
                diag(
                    "E5016",
                    format!("`{name}` is declared twice"),
                    *span,
                    "a screen declares each variable once",
                )
                .with_help("remove one of the two `default` lines"),
            );
            continue;
        }
        seen.push(name);
    }

    // And none of them below the top level: whether a variable exists, and so what it starts as, cannot
    // depend on which arm drew.
    for line in lines {
        check_nested_defaults(line, out);
    }
}

/// Reports every `default` below a screen's top level.
fn check_nested_defaults(line: &ScreenLine, out: &mut Vec<Diagnostic>) {
    for body in nested(line) {
        for line in body {
            if let ScreenLine::Default { span, name, .. } = line {
                out.push(
                    diag(
                        "E5015",
                        format!("`{name}` is declared inside a block"),
                        *span,
                        "a screen variable must be at the top level of the screen",
                    )
                    .with_help("move the `default` above the widget it belongs to"),
                );
            }
            check_nested_defaults(line, out);
        }
    }
}
