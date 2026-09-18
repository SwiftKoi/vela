//! Printing a `test` item.
//!
//! The directives print themselves rather than going through the statement printer, because they are
//! not statements: `expect trust == 1` is an expression with a word in front of it, and `advance 4` is a
//! word and a number. Comments are still honoured — the formatter's promise is that a comment survives
//! everywhere it can be written, including inside a test.

use crate::print::expr;
use crate::print::writer::Writer;
use crate::tree::{Directive, DirectiveKind, TestDecl};

/// Writes a `test` and its directives.
pub(crate) fn test_(writer: &mut Writer<'_>, decl: &TestDecl) {
    writer.line(&format!("test {:?}:", decl.name));
    writer.note(decl.span.start());

    writer.level_up();
    for directive in &decl.directives {
        writer.comments_until(directive.span.start());
        writer.blank_before(directive.span.start());
        writer.line(&text(directive));
    }
    writer.level_down();
}

/// The line one directive prints as.
fn text(directive: &Directive) -> String {
    match &directive.kind {
        DirectiveKind::Run { target, .. } if target.is_empty() => "run".to_string(),
        DirectiveKind::Run { target, .. } => format!("run from {}", target.join(".")),
        DirectiveKind::Advance { count } => format!("advance {count}"),
        DirectiveKind::AdvanceUntil { text } => {
            format!("advance until shown {}", expr::text(text))
        }
        DirectiveKind::Choose { text } => format!("choose {}", expr::text(text)),
        DirectiveKind::Expect { expr } => format!("expect {}", expr::text(expr)),
        DirectiveKind::ExpectShown { text, negated } => {
            let not = if *negated { "not " } else { "" };
            format!("expect {not}shown {}", expr::text(text))
        }
        DirectiveKind::Cover { mode } => format!("cover {}", mode.as_str()),
    }
}
