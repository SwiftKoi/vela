//! Constructing lexical diagnostics.
//!
//! Every constructor names a code from `crates/vela-diag/codes.txt`. Looking the code up
//! by string means a typo fails loudly the first time the path runs, and `check-diag-codes`
//! plus the lexer's own tests mean it cannot ship unregistered.

use vela_diag::{Code, Diagnostic};
use vela_span::{FileId, Span};

use crate::lex::indent::IndentError;

/// Builds a diagnostic for a registered code.
///
/// # Panics
///
/// Panics if `code` is not registered. That is a bug in this crate rather than anything
/// a user can trigger: `check-diag-codes` rejects an unregistered code before it can be
/// committed, and every constructor below is exercised by a test.
fn diag(
    code: &str,
    message: impl Into<String>,
    span: Span,
    label: impl Into<String>,
) -> Diagnostic {
    let code =
        Code::new(code).unwrap_or_else(|| panic!("`{code}` is not in crates/vela-diag/codes.txt"));
    Diagnostic::new(code, message, span, label)
}

/// `E0001` — a byte order mark.
#[must_use]
pub fn bom(file: FileId) -> Diagnostic {
    diag(
        "E0001",
        "byte order mark is not permitted",
        Span::new(file, 0, 3),
        "remove this; Vela source is UTF-8 without a BOM",
    )
}

/// `E0002` — a carriage return with no line feed.
#[must_use]
pub fn lone_carriage_return(file: FileId, offset: u32) -> Diagnostic {
    diag(
        "E0002",
        "carriage return without a line feed",
        Span::new(file, offset, offset + 1),
        "line endings must be `\\n` or `\\r\\n`",
    )
}

/// `E0003` — a tab in indentation.
#[must_use]
pub fn tab_indentation(file: FileId, offset: u32) -> Diagnostic {
    diag(
        "E0003",
        "tab used for indentation",
        Span::new(file, offset, offset + 1),
        "indentation must be spaces, so that a block's width is unambiguous",
    )
}

/// `E0004` or `E0005` — a problem reconciling this line's indentation.
#[must_use]
pub fn indentation(file: FileId, start: u32, end: u32, error: IndentError) -> Diagnostic {
    let span = Span::new(file, start, end);
    match error {
        IndentError::InconsistentStep { expected, found } => diag(
            "E0004",
            "inconsistent indentation width in this block",
            span,
            format!("this block indents by {expected} columns; this line indents by {found}"),
        ),
        IndentError::MismatchedDedent { found } => diag(
            "E0005",
            "dedent does not match any enclosing indentation level",
            span,
            format!("this line starts at column {found}, which is not an open block"),
        ),
    }
}

/// `E0006` — a numeric literal that is malformed or does not fit.
#[must_use]
pub fn numeric_overflow(file: FileId, start: u32, end: u32) -> Diagnostic {
    diag(
        "E0006",
        "numeric literal is malformed or out of range",
        Span::new(file, start, end),
        "this literal is not a valid number of its kind",
    )
}

/// `E0007` — a float literal ending in a decimal point.
#[must_use]
pub fn trailing_decimal_point(file: FileId, start: u32, end: u32) -> Diagnostic {
    diag(
        "E0007",
        "float literal ends with a decimal point",
        Span::new(file, start, end),
        "write a digit after the point, or remove it",
    )
}

/// `E0008` — a string literal with no closing quote.
#[must_use]
pub fn unterminated_string(file: FileId, start: u32, end: u32) -> Diagnostic {
    diag(
        "E0008",
        "unterminated string literal",
        Span::new(file, start, end),
        "this string is missing its closing `\"`",
    )
}

/// `E0009` — a byte the lexer does not recognise.
#[must_use]
pub fn unexpected_character(file: FileId, offset: u32) -> Diagnostic {
    diag(
        "E0009",
        "unexpected character",
        Span::new(file, offset, offset + 1),
        "this character is not part of the language",
    )
}

/// `E1001` — a token that does not fit where it was found.
#[must_use]
pub fn unexpected(span: Span, found: &str, expected: &str) -> Diagnostic {
    diag(
        "E1001",
        format!("expected {expected}, found {found}"),
        span,
        format!("expected {expected}"),
    )
}

/// `E1002` — a `:` with no indented block after it.
#[must_use]
pub fn expected_block(span: Span, construct: &str) -> Diagnostic {
    diag(
        "E1002",
        format!("expected an indented block after {construct}"),
        span,
        "add an indented block here",
    )
}

/// `E1003` — an expression was expected.
#[must_use]
pub fn expected_expression(span: Span, found: &str) -> Diagnostic {
    diag(
        "E1003",
        "expected an expression",
        span,
        format!("found {found}"),
    )
}

/// `E1004` — a name was expected.
#[must_use]
pub fn expected_name(span: Span, after: &str) -> Diagnostic {
    diag(
        "E1004",
        format!("expected a name after `{after}`"),
        span,
        "expected a name here",
    )
}

/// `E1005` — a type was expected.
#[must_use]
pub fn expected_type(span: Span, found: &str) -> Diagnostic {
    diag("E1005", "expected a type", span, format!("found {found}"))
}
