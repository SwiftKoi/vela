//! Checking `@"path"` literals against the project's asset manifest.
//!
//! `LANGUAGE.md §7.5`: *"The compiler resolves it against the project's asset manifest at build
//! time; a missing asset is `E7001`."* The manifest is the build's, not the compiler's — so the
//! known sources arrive through [`Session::set_assets`](crate::Session::set_assets) and this
//! module only answers the question *given* them.
//!
//! Why this is worth a pass of its own: the bug it catches is the one that works on the machine
//! the art was added on and shows a blank rectangle on everybody else's.

use std::collections::BTreeSet;

use vela_diag::{Code, Diagnostic};
use vela_span::Span;
use vela_syntax::PathRef;

/// `E7001` — a path literal names an asset the manifest does not have.
///
/// # Panics
///
/// Panics if the code is not registered. That is a bug here rather than anything a user can
/// trigger: `check-diag-codes` rejects an unregistered code before it can be committed.
#[must_use]
pub fn missing_asset(path: &str, span: Span) -> Diagnostic {
    let code = Code::new("E7001")
        .unwrap_or_else(|| panic!("`E7001` is not in crates/vela-diag/codes.txt"));

    Diagnostic::new(
        code,
        format!("no asset at `{path}`"),
        span,
        "not one of this project's assets",
    )
    .with_help(
        "assets are imported from `assets/`; the manifest `@\"path\"` is resolved against is \
         what `vela build` writes",
    )
}

/// Every path literal that names nothing the project has.
///
/// One diagnostic per literal, in the order the literals were found, and a file may report
/// several — an author who renamed a directory has several to fix, and reporting the first
/// would make that a sequence of builds instead of a list.
#[must_use]
pub fn missing_assets(literals: &[PathRef], known: &BTreeSet<String>) -> Vec<Diagnostic> {
    literals
        .iter()
        .filter(|literal| !known.contains(&literal.value))
        .map(|literal| missing_asset(&literal.value, literal.span))
        .collect()
}
