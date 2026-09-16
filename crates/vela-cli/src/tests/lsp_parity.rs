//! The editor and the command line must say the same thing about the same file.
//!
//! `TOOLING.md §4` and `M10-tooling.md` make this a criterion rather than a hope: "diagnostics must
//! equal `vela check` output exactly, verified by a parity test". An editor that disagrees with CI is
//! worse than no editor — the same file is clean in one window and broken in the other, and neither
//! answer looks wrong on its own.
//!
//! Both sides ask `vela_lsp::diagnostics`, so the parity below is a property of the *call*: what these
//! tests guard is that `vela check` has not grown a list of its own again. They compare the command's
//! actual output — the JSON a tool would read — against the same renderer over the language server's
//! answer, which is the only comparison that would notice a second list appearing.

use super::support::cli;
use crate::format;

/// A project with one problem of each kind the front end can find.
///
/// A type error in one file, an unreachable label so the whole-program pass has something to say, and
/// a screen warning — the three *origins* of a diagnostic, which is what a parity test has to cover:
/// two lists that agree only on the compiler's findings would agree about most files.
const SOURCE: &str = "\
struct Route:
    name: str

default who: str = \"Ren\"

label start:
    var r = Route
    var s = \"route [r]\"
    \"And an unreachable one below.\"
    return

label never_reached:
    return

screen pause:
    layer ui
    button:
        text \"[who]\"
";

/// The project's diagnostics as the language server produces them, rendered the way the CLI renders.
fn lsp_json(project: &std::path::Path) -> String {
    let collected = crate::commands::check::collect(project).expect("the fixture collects");
    let mut session = crate::commands::check::load(&collected).expect("the fixture loads");
    let diagnostics = vela_lsp::diagnostics::project(&mut session);

    format::json(&diagnostics, session.sources()).to_string()
}

#[test]
fn the_command_and_the_language_server_report_the_same_diagnostics() {
    let project = super::support::temp_project("lsp-parity", SOURCE);

    let (code, out) = cli(&["check", "--format", "json", &project.to_string_lossy()]);
    assert_eq!(code, 1, "the fixture is meant to have errors: {out}");

    assert_eq!(
        out.trim(),
        lsp_json(&project).trim(),
        "the editor and the command line disagree about this project"
    );
}

/// The other half of the same property: asking per file — which is what an editor does after an edit —
/// must cover exactly the project's findings, or a diagnostic would be visible to one caller and
/// invisible to the other.
///
/// Compared as a *multiset*, and that is the honest statement: the two callers want different orders.
/// `vela check` prints the compiler's findings, then the whole-program ones, then the screens, and an
/// editor publishes one document at a time. What must match is which diagnostics exist, and that each
/// per-file answer is in source order — which the second half of this test checks.
///
/// Note what is *not* added here: the whole-program findings. `file` already carries the ones that
/// point at the file, which is exactly what makes publishing one document at a time complete — and
/// adding them again here would be the test asserting a belief the server does not hold.
#[test]
fn per_file_answers_cover_the_project() {
    let project = super::support::temp_project("lsp-parity-files", SOURCE);
    let collected = crate::commands::check::collect(&project).expect("the fixture collects");
    let mut session = crate::commands::check::load(&collected).expect("the fixture loads");

    let whole = vela_lsp::diagnostics::project(&mut session);

    let mut per_file = Vec::new();
    for file in session.file_ids() {
        let one = vela_lsp::diagnostics::file(&mut session, file);
        let starts: Vec<u32> = one
            .iter()
            .map(|diagnostic| diagnostic.primary.span.start())
            .collect();
        assert!(
            starts.windows(2).all(|pair| pair[0] <= pair[1]),
            "a per-file answer is not in source order: {starts:?}"
        );
        per_file.extend(one);
    }

    let mut listed: Vec<(String, u32)> = whole.iter().map(identify).collect();
    let mut per: Vec<(String, u32)> = per_file.iter().map(identify).collect();
    listed.sort();
    per.sort();

    assert_eq!(listed, per, "a per-file answer is missing something");
}

/// A diagnostic's identity for a set comparison: what it says and where.
fn identify(diagnostic: &vela_diag::Diagnostic) -> (String, u32) {
    (
        diagnostic.primary.span.file().as_raw().to_string() + ":" + diagnostic.code.as_str(),
        diagnostic.primary.span.start(),
    )
}

/// And it is the answer that matters, not just a consistent one: the fixture is meant to produce a
/// type error, a whole-program finding, and a screen warning.
#[test]
fn the_fixture_exercises_every_origin() {
    let project = super::support::temp_project("lsp-parity-origins", SOURCE);
    let collected = crate::commands::check::collect(&project).expect("the fixture collects");
    let mut session = crate::commands::check::load(&collected).expect("the fixture loads");
    let diagnostics = vela_lsp::diagnostics::project(&mut session);

    let codes: Vec<&str> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect();

    assert!(codes.contains(&"E3005"), "a type error: {codes:?}");
    assert!(codes.contains(&"W4002"), "an unreachable label: {codes:?}");
    assert!(codes.contains(&"W4010"), "a screen warning: {codes:?}");
}
