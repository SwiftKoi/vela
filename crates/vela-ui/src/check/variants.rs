//! `E5018` — a `variant` call that does not name a variant.

use vela_diag::Diagnostic;
use vela_span::Span;
use vela_syntax::{Expr, StrPart};

use crate::variants::Variant;

use super::diag;

/// Checks the name one `variant(...)` question asks about (`SCREENS.md §2.6`).
///
/// The name has to be *written out* as a literal, because the vocabulary is closed and a name the
/// engine cannot see cannot be checked — and a checked name is what this buys over Ren'Py, which
/// answers `False` for every name it does not have. A migration that needs a name Vela does not have
/// (`tablet`, `touch`, `tv`) therefore hears about it here rather than drawing the `else` arm forever.
///
/// Called from both walks that visit expressions, because the positions they cover are disjoint: the
/// action walk sees a prop or an argument, the condition walk sees an `if`, and a question may be
/// written in either (`check/actions.rs`, `check/conditions.rs`).
pub(super) fn check_variant(args: &[Expr], span: Span, out: &mut Vec<Diagnostic>) {
    let known: Vec<&str> = Variant::ALL.iter().map(|v| v.name()).collect();
    match written_name(args) {
        Some(name) if Variant::named(name).is_some() => {}
        Some(name) => out.push(
            diag(
                "E5018",
                format!("`{name}` is not a variant"),
                span,
                format!("this build knows {}", known.join(", ")),
            )
            .with_help(
                "a variant names the platform a bundle was built for, or the room a frame has",
            ),
        ),
        None => out.push(
            diag(
                "E5018",
                "`variant` needs the name written out".to_string(),
                span,
                "a variant name is checked where it is written",
            )
            .with_help(format!("write one of: {}", known.join(", "))),
        ),
    }
}

/// The variant name a call writes, if it writes one rather than computing it.
///
/// Exactly one argument, exactly one literal part: `variant("p{name}c")` is a name the engine cannot
/// see whole, and a name that cannot be seen whole cannot be checked.
fn written_name(args: &[Expr]) -> Option<&str> {
    match args {
        [Expr::Str { parts, .. }] => match parts.as_slice() {
            [StrPart::Literal { text, .. }] => Some(text.as_str()),
            _ => None,
        },
        _ => None,
    }
}
