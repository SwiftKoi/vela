//! Everything the compiler and the widgets have to say about a project.
//!
//! # Why this is one function
//!
//! `vela check` and the editor must agree *exactly* — `TOOLING.md §4` and `M10-tooling.md` make that
//! a criterion, and the reason is that an editor which disagrees with CI is worse than no editor:
//! the same file is clean in one window and broken in the other, and neither is obviously wrong. So
//! the answer is assembled in one place and both callers ask it. The CLI renders it, the language
//! server maps it onto LSP ranges, and neither decides what is in it.
//!
//! # Why screens are here and not in `vela-compile`
//!
//! The screen checks need the widget vocabulary, which is `vela-ui` (rank 8), and the query database
//! is `vela-compile` (rank 7): a crate can see both only from rank 9 up. That is why this crate is
//! rank 9 and not 8 — at rank 8 it could not reach `vela-ui` at all, and "the LSP publishes what
//! `vela check` publishes" would have been impossible rather than merely untested. The comment in
//! `vela-cli` that claimed the language server could check screens was the tell: the criterion was
//! written down before the ranks made it satisfiable.
//!
//! # What is *not* decided here
//!
//! Per-file questions are answered per file, because that is what an editor needs: an edit re-checks
//! the module it touched and the ones that name it, and nothing else (`vela-compile`'s whole design).
//! The whole-program findings — a label nothing reaches — are answered for the project, since no
//! subset of modules can answer them.

use vela_compile::Session;
use vela_diag::Diagnostic;
use vela_syntax::{Item, ParseResult, StyleDecl};

/// Every diagnostic for every file in the session, in the order `vela check` prints them.
///
/// The compiler's findings first, in file order, then the whole-program ones, then the screens: the
/// same order `vela-compile` uses, and stable so that a diff of a check's output means something.
#[must_use]
pub fn project(session: &mut Session) -> Vec<Diagnostic> {
    let mut diagnostics = session.diagnostics();

    // Screens are checked per file against the styles *in that file* (`SCREENS.md §5`), which is what
    // makes this a per-file question rather than one more whole-program pass.
    for file in session.file_ids() {
        let parsed = session.parse(file);
        diagnostics.extend(screens(&parsed));
    }
    diagnostics
}

/// One file's diagnostics: what the compiler says about it, and what its screens say.
///
/// This is the editor's question after an edit. It deliberately leaves out the whole-program findings:
/// a label nothing reaches is not a fact about the file that was just typed in, and re-reporting it in
/// every open document would say so anyway.
#[must_use]
pub fn file(session: &mut Session, file: vela_span::FileId) -> Vec<Diagnostic> {
    let mut diagnostics = session.check(file).as_ref().clone();
    let parsed = session.parse(file);
    diagnostics.extend(screens(&parsed));
    diagnostics
}

/// The screen diagnostics of one parsed file.
///
/// Takes the *parsed* file rather than its text so the spans carry the file they came from. Reading
/// the text again and parsing it with a fixed id — which is what this replaced — told every screen
/// warning in a project that it belonged to the first file, so a warning about a screen in
/// `chapters/street.vela` was rendered against `main.vela`, at whatever line happened to be there.
fn screens(parsed: &ParseResult) -> Vec<Diagnostic> {
    let registry = vela_ui::WidgetRegistry::builtin();
    let styles: Vec<&StyleDecl> = parsed
        .program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Style(style) => Some(style),
            _ => None,
        })
        .collect();

    let mut diagnostics = vela_ui::check_inheritance(&styles);
    for item in &parsed.program.items {
        let Item::Screen(screen) = item else {
            continue;
        };
        diagnostics.extend(vela_ui::check_screen(&screen.body, &registry));
        diagnostics.extend(vela_ui::check_screen_styles(&screen.body, &styles));
        diagnostics.extend(vela_ui::a11y::check_labels(&screen.body, &registry));
        diagnostics.extend(vela_ui::check_magic_colours(&screen.body));
    }
    diagnostics
}
