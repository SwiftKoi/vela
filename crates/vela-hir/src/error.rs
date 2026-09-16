//! Constructing name-resolution diagnostics.

use vela_diag::{Code, Diagnostic};
use vela_span::Span;
use vela_syntax::Pragma;

use crate::def::Definition;
use crate::module::ModuleName;

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

/// `E2003` — a name defined twice in one module.
#[must_use]
pub fn duplicate_definition(
    module: &ModuleName,
    definition: &Definition,
    previous: &Definition,
) -> Diagnostic {
    diag(
        "E2003",
        format!(
            "`{}` is defined more than once in `{module}`",
            definition.name
        ),
        definition.span,
        format!("defined again here, as {}", definition.kind.describe()),
    )
    .with_secondary(
        previous.span,
        format!("first defined here as {}", previous.kind.describe()),
    )
}

/// `E2002` — a reference names a module this one has not imported.
#[must_use]
pub fn not_imported(module_path: &str, span: Span) -> Diagnostic {
    diag(
        "E2002",
        format!("`{module_path}` is not imported"),
        span,
        "this module has no `use` for it",
    )
    .with_help(format!("add `use {module_path}`"))
}

/// `E2005` — a qualified name names a *value* in another module.
///
/// `jump forest.clearing` is a label reference and resolves; `forest.helper(2)` cannot, because
/// the checker is per-module and has no signature to check the call against. Reported here rather
/// than left to run: without it the call lowers to a field read on nothing and faults the first
/// time a player reaches it, which is the worst place to learn.
#[must_use]
pub fn module_value(qualified: &str, module: &str, span: Span) -> Diagnostic {
    diag(
        "E2005",
        format!("`{qualified}` is a value in `{module}`"),
        span,
        "only another module's labels can be referenced, not its values",
    )
    .with_help("`jump` or `call` one of its labels, or declare what you need in this module")
}

/// `E2001` — a name that means nothing here.
#[must_use]
pub fn undefined_name(name: &str, span: Span) -> Diagnostic {
    diag(
        "E2001",
        format!("undefined name `{name}`"),
        span,
        "nothing with this name is in scope",
    )
}

/// `E2001` — a `use` naming a module that does not exist.
#[must_use]
pub fn unknown_module(path: &str, span: Span) -> Diagnostic {
    diag(
        "E2001",
        format!("no module `{path}`"),
        span,
        "nothing in this project has that name",
    )
}

/// `E5001` — a say statement attributed to something that is not a character.
#[must_use]
pub fn undefined_character(name: &str, span: Span) -> Diagnostic {
    diag(
        "E5001",
        format!("`{name}` is not a character"),
        span,
        "a speaker has to be a declared `character`",
    )
    .with_help(format!("declare it, as in `character {name}:`"))
}

/// `W4002` — a label no path from an entry point can reach.
#[must_use]
pub fn unreachable_label(name: &str, span: Span, module: &str) -> Diagnostic {
    diag(
        "W4002",
        format!("unreachable label `{name}`"),
        span,
        format!("nothing in `{module}` transfers here"),
    )
}

/// `W4011` — a formatting pragma, reported so that it is visible in review.
#[must_use]
pub fn format_pragma(pragma: Pragma, span: Span) -> Diagnostic {
    match pragma {
        Pragma::Off => diag(
            "W4011",
            "`# fmt: off` leaves this region as written",
            span,
            "the formatter will not reformat from here",
        )
        .with_help("allowed, and worth a comment saying why; remove it when the region no longer needs hand layout"),
        Pragma::On => diag(
            "W4011",
            "`# fmt: on` closes a region the formatter leaves alone",
            span,
            "the region above is reproduced as written",
        )
        .with_help("if nothing above is a `# fmt: off`, this does nothing"),
    }
}

/// `W4003` — a label some path can fall off the end of.
#[must_use]
pub fn label_falls_through(name: &str, span: Span) -> Diagnostic {
    diag(
        "W4003",
        format!("label `{name}` can end without transferring"),
        span,
        "this path reaches the end of the label",
    )
    .with_help("add a `jump`, `call`, or `return`")
}

/// `E5003` — a `jump` or `call` to a label that does not exist.
///
/// The position alone does not help an author: knowing *where* the typo is and not what it
/// should have been turns a fix into a hunt. So this offers the nearest name when one is
/// close enough, and lists what the module actually has — bounded, because a note that
/// names two hundred labels is noise rather than information.
#[must_use]
pub fn undefined_label(path: &str, span: Span, module: &str, labels: &[String]) -> Diagnostic {
    let mut diagnostic = diag(
        "E5003",
        format!("undefined label `{path}`"),
        span,
        format!("no label `{path}` in `{module}`"),
    );

    // Compared on the last segment: `forest.clearring` is a typo in `clearring`, and the
    // suggestion has to come back in the form the author wrote it — a bare `clearing` in a
    // qualified jump would be a second mistake.
    let (prefix, leaf) = match path.rsplit_once('.') {
        Some((prefix, leaf)) => (Some(prefix), leaf),
        None => (None, path),
    };
    let candidates = labels.iter().map(String::as_str);
    if let Some(nearest) = vela_diag::closest(leaf, candidates) {
        let suggestion = match prefix {
            Some(prefix) => format!("{prefix}.{nearest}"),
            None => nearest,
        };
        diagnostic = diagnostic.with_help(format!("did you mean `{suggestion}`?"));
    }

    if !labels.is_empty() {
        diagnostic = diagnostic.with_note(format!("labels in `{module}`: {}", summarise(labels)));
    }
    diagnostic
}

/// How many names a note lists before it stops being a list and starts being noise.
const NOTE_LIMIT: usize = 8;

/// A module's labels, or as many as are worth printing.
fn summarise(labels: &[String]) -> String {
    if labels.len() <= NOTE_LIMIT {
        return labels.join(", ");
    }
    let shown = labels[..NOTE_LIMIT].join(", ");
    format!("{shown}, and {} more", labels.len() - NOTE_LIMIT)
}
