//! Diagnostics lowering reports.
//!
//! Lowering is the last place a *source* problem can be caught: everything after it is
//! MIR, where the span table is the only link back to what an author wrote.

use vela_diag::{Code, Diagnostic};
use vela_span::Span;

/// Builds a diagnostic for a registered code.
///
/// # Panics
///
/// Panics if the code is not registered. `check-diag-codes` rejects an unregistered code
/// before it can be committed.
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

/// `E2004` — a `const` or `default` initialised with something that is not constant.
///
/// Reported rather than worked around because the alternative is silent: a `default` whose
/// value could not be computed at compile time would start as nothing, and a save schema
/// that quietly loses a field is exactly the class of bug `LANGUAGE.md §7.2` exists to
/// prevent.
#[must_use]
pub fn not_constant(name: &str, span: Span) -> Diagnostic {
    diag(
        "E2004",
        format!("`{name}` is not initialised with a constant"),
        span,
        "this has to be a value the compiler knows: a literal, arithmetic over literals, or another `const`",
    )
    .with_help("move the computation into a `fn` and call it from story code instead")
}
