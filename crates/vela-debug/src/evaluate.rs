//! Evaluating an expression in a paused frame, with the real checker.
//!
//! `TOOLING.md §6` makes this a *checker* question, not an evaluator question: an invalid
//! expression must be the same `Exxx` the editor would show, never a crash inside a second
//! interpreter. So this file does exactly what the compiler does to the expression — parse it,
//! resolve its names, type it — and reports what the checker said.
//!
//! # Why the expression is wrapped rather than parsed alone
//!
//! The language has no standalone-expression grammar: an expression is only meaningful inside a
//! body. So it is wrapped in a function appended to the file's own text, the same way
//! `vela-test` wraps an assertion (`vela-test/src/plan.rs`) — the wrapper goes through the same
//! parser, the same name resolution, and the same checker as everything else, and nothing
//! re-prints the expression, so nothing can print it differently from how it was written. The
//! function is never run; only its checking is read.
//!
//! # Why the answer is read off the wrapper's variable
//!
//! The type of the *whole* expression is wanted, and `vela_types::at` answers about the
//! innermost expression under an offset — which for `a + b` is `a`. A `var` declaration is
//! recorded against its own name with the type inferred from its initialiser, so asking about the
//! wrapper's variable name asks about the whole expression.

use vela_diag::{Diagnostic, Severity};
use vela_hir::ModuleName;
use vela_span::FileId;

/// The variable the wrapped expression is assigned to. Its name is what the type is read from.
const MARKER: &str = "__vela_evaluate_value";

/// Checks `expression` against `text`, with `locals` in scope, and returns its type.
///
/// `locals` are the paused frame's named slots as `(name, type)` in source form — the debugger's
/// variable view and its expression scope have to be the same names, or a name shown in the view
/// would be unknown to an expression that mentions it.
///
/// # Errors
///
/// Returns the checker's own diagnostics, formatted as `Exxx: message`, when the expression does
/// not check. That is the whole point of the function: a bad expression is a diagnostic, not a
/// crash.
pub fn evaluate(
    file: FileId,
    name: &str,
    text: &str,
    expression: &str,
    locals: &[(String, String)],
) -> Result<String, Vec<String>> {
    let (source, marker) = wrap(text, expression, locals);
    let parsed = vela_syntax::parse(file, &source);

    let mut errors = problems(&parsed.diagnostics);

    let collected = vela_hir::collect(ModuleName::new(name), file, &parsed.program);
    errors.extend(problems(&collected.diagnostics));
    errors.extend(problems(&vela_hir::resolve_names(
        &collected.module,
        &parsed.program,
    )));

    let (env, from_env) = vela_types::Env::build(&parsed.program);
    errors.extend(problems(&from_env));
    errors.extend(problems(&vela_types::check(&parsed.program, &env)));

    if !errors.is_empty() {
        return Err(errors);
    }

    // Asking about the wrapper's variable gives the type of the whole expression, whatever shape
    // it had.
    Ok(vela_types::at(&parsed.program, &env, marker)
        .map(|found| found.ty().to_string())
        .unwrap_or_else(|| "?".to_string()))
}

/// The file's text with a function that evaluates the expression appended, and where that
/// function's variable name sits.
fn wrap(text: &str, expression: &str, locals: &[(String, String)]) -> (String, u32) {
    let params = locals
        .iter()
        .map(|(name, ty)| format!("{name}: {ty}"))
        .collect::<Vec<_>>()
        .join(", ");
    let one_line = expression.split_whitespace().collect::<Vec<_>>().join(" ");
    let wrapper = format!(
        "\nfn __vela_evaluate({params}) -> int:\n    var {MARKER} = {one_line}\n    return 0\n"
    );

    let marker = u32::try_from(text.len() + wrapper.find(MARKER).unwrap_or(0)).unwrap_or(u32::MAX);
    let mut source = String::with_capacity(text.len() + wrapper.len());
    source.push_str(text);
    source.push_str(&wrapper);
    (source, marker)
}

/// The errors in a diagnostic list, as `Exxx: message`.
///
/// Warnings are left out deliberately: the wrapper itself can warn (an unused parameter, an
/// unused variable), and those are about the wrapper rather than about the expression.
fn problems(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity() == Severity::Error)
        .map(|diagnostic| format!("{}: {}", diagnostic.code.as_str(), diagnostic.message))
        .collect()
}
