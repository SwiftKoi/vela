//! Constructing type diagnostics.

use vela_diag::{Code, Diagnostic};
use vela_span::Span;

use crate::ty::Ty;

/// Builds a diagnostic for a registered code.
///
/// # Panics
///
/// Panics if the code is not registered. That is a bug here rather than anything a user
/// can trigger: `check-diag-codes` rejects an unregistered code before it can be
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

/// `E3001` — an enum with no variants.
#[must_use]
pub fn empty_enum(name: &str, span: Span) -> Diagnostic {
    diag(
        "E3001",
        format!("enum `{name}` has no variants"),
        span,
        "an enum with no variants has no values",
    )
    .with_help("add at least one variant")
}

/// `E3002` — an optional used where its payload is required.
#[must_use]
pub fn needs_unwrap(expected: &Ty, span: Span) -> Diagnostic {
    diag(
        "E3002",
        format!("expected `{expected}`, found an optional"),
        span,
        "this value may be `none`",
    )
    .with_help("unwrap it with `??`, or match on it")
}

/// `E3003` — a declaration that needs to say what type it holds.
#[must_use]
pub fn needs_annotation(name: &str, span: Span) -> Diagnostic {
    diag(
        "E3003",
        format!("`{name}` needs a type"),
        span,
        "this value's type is not obvious from the expression",
    )
    .with_help("write the type, as in `default score: int = …`")
}

/// `E3004` — branches of a conditional that do not agree on a type.
#[must_use]
pub fn branch_mismatch(first: &Ty, second: &Ty, span: Span) -> Diagnostic {
    diag(
        "E3004",
        format!("branches have different types: `{first}` and `{second}`"),
        span,
        format!("this branch is `{second}`"),
    )
    .with_help("give both branches the same type, or annotate the declaration")
}

/// `E3005` — a value that cannot be interpolated into a string.
#[must_use]
pub fn not_displayable(found: &Ty, span: Span) -> Diagnostic {
    diag(
        "E3005",
        format!("`{found}` cannot be interpolated"),
        span,
        "only values with an obvious rendering can",
    )
    .with_help("convert it explicitly, as in `str(…)`")
}

/// `E3006` — arithmetic mixing `int` and `float`.
#[must_use]
pub fn mixed_numbers(left: &Ty, right: &Ty, span: Span) -> Diagnostic {
    diag(
        "E3006",
        format!("cannot combine `{left}` and `{right}`"),
        span,
        "there is no implicit conversion between them",
    )
    .with_help("convert one explicitly with `int(…)` or `float(…)`")
}

/// `E4001` — a `match` that does not cover every case.
#[must_use]
pub fn non_exhaustive(missing: &[String], span: Span) -> Diagnostic {
    diag(
        "E4001",
        format!("no arm for {}", missing.join(", ")),
        span,
        "this match does not cover every case",
    )
    .with_help("add the missing arms, or an `else`")
}

/// `E4002` — a function that can finish without returning a value.
#[must_use]
pub fn missing_return(name: &str, span: Span, ret: &Ty) -> Diagnostic {
    diag(
        "E4002",
        format!("`{name}` does not always return `{ret}`"),
        span,
        "this path reaches the end without returning",
    )
    .with_help("return a value on every path")
}

/// `W4005` — an arm that can never be reached.
#[must_use]
pub fn unreachable_arm(span: Span) -> Diagnostic {
    diag(
        "W4005",
        "this arm can never be reached",
        span,
        "an earlier arm already covers everything",
    )
    .with_help("remove it, or move it above the catch-all")
}

/// `W4006` — a condition whose value is already known.
#[must_use]
pub fn constant_condition(span: Span) -> Diagnostic {
    diag(
        "W4006",
        "this condition is constant",
        span,
        "the branch is decided here rather than at run time",
    )
    .with_help("remove the `if`, or use the value you meant to test")
}

/// `E3008` — a call with the wrong number of arguments.
///
/// Separate from `E3007` because "the wrong type" and "the wrong count" are different
/// mistakes, and the second is the one that happens when a function gains a parameter.
#[must_use]
pub fn wrong_arity(expected: usize, found: usize, span: Span) -> Diagnostic {
    diag(
        "E3008",
        format!("this takes {expected} argument(s) but was given {found}"),
        span,
        "the arguments do not match the declaration",
    )
}

/// A value whose type does not fit where it is used.
#[must_use]
pub fn mismatch(expected: &Ty, found: &Ty, span: Span) -> Diagnostic {
    diag(
        "E3007",
        format!("expected `{expected}`, found `{found}`"),
        span,
        format!("this is `{found}`"),
    )
}
