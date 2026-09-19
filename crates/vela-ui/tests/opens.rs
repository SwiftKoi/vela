//! `open_screen`'s target: a screen the whole *game* declares (`SCREENS.md §2.1`).
//!
//! The one screen question that is not per file, so these fixtures are *projects*: one source string per
//! file, and the point of most of them is that the target lives in a different file from the call.
//!
//! What this replaces is a silence worth naming. `ShowMenu("preferences")` migrates to
//! `open_screen("preferences")`, and until this check existed nothing held that name to anything: a
//! migrated project whose app screens were renamed, or a hand-written button with a typo in it, produced a
//! project that checked clean and a button that did nothing at all — the exact failure class the settings
//! vocabulary was added for.

use vela_span::FileId;
use vela_syntax::{Item, ParseResult, ScreenDecl, parse};
use vela_ui::check_open_screens;

/// A parsed file, for the two passes a project needs.
fn parsed(source: &str) -> ParseResult {
    let parsed = parse(FileId::from_raw(0), source);
    assert!(
        parsed.diagnostics.is_empty(),
        "the fixture must parse: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| d.message.clone())
            .collect::<Vec<_>>()
    );
    parsed
}

/// Every screen a parsed file declares.
fn declares(parsed: &ParseResult) -> Vec<&ScreenDecl> {
    parsed
        .program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Screen(screen) => Some(screen),
            _ => None,
        })
        .collect()
}

/// Every diagnostic a project of one source per file produces.
fn diagnostics(files: &[&str]) -> Vec<vela_diag::Diagnostic> {
    let parsed: Vec<ParseResult> = files.iter().map(|source| parsed(source)).collect();
    let declared: Vec<Vec<&ScreenDecl>> = parsed.iter().map(declares).collect();
    let project: Vec<&[&ScreenDecl]> = declared.iter().map(Vec::as_slice).collect();
    check_open_screens(&project)
}

/// Checks a project, returning `(code, message)` per diagnostic.
fn diagnose(files: &[&str]) -> Vec<(String, String)> {
    diagnostics(files)
        .into_iter()
        .map(|diagnostic| (diagnostic.code.as_str().to_string(), diagnostic.message))
        .collect()
}

/// The label of each diagnostic, which is where the sentence about *what is wrong* lives.
fn label(files: &[&str]) -> Vec<(String, String)> {
    diagnostics(files)
        .into_iter()
        .map(|diagnostic| {
            (
                diagnostic.code.as_str().to_string(),
                diagnostic.primary.message.clone(),
            )
        })
        .collect()
}

/// A screen that opens another, and the other in a file of its own.
///
/// The string spelling is what this check reads, so the fixtures write it that way — the migration emits
/// it, and it is the rule `SCREENS.md §7.1` states for a name in another vocabulary.
#[test]
fn a_target_in_another_file_is_the_point() {
    let menu = "screen pause:\n    column:\n        button:\n            text \"Settings\"\n            action open_screen(\"settings\")\n";
    let settings = "screen settings():\n    text \"Settings\"\n";

    assert!(diagnose(&[menu, settings]).is_empty());
    // And the same call with the other file missing is what this check is for: a press that would do
    // nothing, reported where it was written.
    assert_eq!(diagnose(&[menu])[0].0, "E5009", "a target no file declares");
    assert!(
        diagnose(&[menu])[0].1.contains("`settings`"),
        "{:?}",
        diagnose(&[menu])
    );
}

/// An argument that does not fit the opened screen is `E5010`, the code a `use` answers to.
#[test]
fn arguments_that_do_not_fit_the_opened_screen_are_reported() {
    let menu = "screen pause:\n    button:\n        text \"Detail\"\n        action open_screen(\"detail\", 1, 2, 3)\n";
    let detail = "screen detail(title, page = 2):\n    text title\n";

    let found = diagnose(&[menu, detail]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "E5010");
    // The *label* is where the count lives — the message names the call, as it does for a `use` — so the
    // sentence a reader meets is the same one either way.
    assert_eq!(
        found[0].1, "`detail` cannot be called this way",
        "{found:?}"
    );
    let labelled = label(&[menu, detail]);
    assert_eq!(labelled[0].1, "it takes 2 parameter(s), but 3 were given");

    // Fewer than it takes is fine: a parameter the call omits keeps its default, and one with no default
    // is the callee's problem rather than a call that cannot mean anything.
    let menu = "screen pause:\n    button:\n        text \"Detail\"\n        action open_screen(\"detail\", 1)\n";
    assert!(diagnose(&[menu, detail]).is_empty());
}

/// A target the screen *computes* is not a string, so it is somebody else's question.
#[test]
fn a_computed_target_is_not_reported() {
    // A parameter, a variable, and a loop binding are values rather than names to look up — the evaluator
    // resolves them, so which screen a call opens is the runtime's to find.
    let source = "\
screen pause(which, slots):
    default fallback = \"nothing\"
    column:
        button:
            text \"By parameter\"
            action open_screen(which)
        button:
            text \"By variable\"
            action open_screen(fallback)
        for name in slots:
            button:
                text name
                action open_screen(name)
";
    assert!(diagnose(&[source]).is_empty(), "{:?}", diagnose(&[source]));
}

/// A bare target is `E2001`'s, and this check stays quiet about it.
///
/// One mistake, one diagnostic: the language's name resolution already reports a bare name that resolves
/// to nothing, and this check's sentence would be the second. What a bare name *can* be is a value the
/// screen computed — `open_screen(which)`, with `which` a parameter — which is why it is nobody's here.
#[test]
fn a_bare_target_is_left_to_the_name_check() {
    let menu = "screen pause(which):\n    button:\n        text \"Go\"\n        action open_screen(settingss)\n    button:\n        text \"By parameter\"\n        action open_screen(which)\n";
    assert!(diagnose(&[menu]).is_empty(), "{:?}", diagnose(&[menu]));
}

/// A screen the interface declares is a target like any other (`SCREENS.md §2.7`).
///
/// This is the case that found the bug: `open_screen("preferences")` was reported as a name nothing
/// declares, because the check looked at the project's files and not at the interface — the failure this
/// check exists to catch, reported on a project that was right.
#[test]
fn an_interface_screen_is_a_target() {
    let menu = "screen pause:\n    button:\n        text \"Preferences\"\n        action open_screen(\"preferences\")\n";
    assert!(diagnose(&[menu]).is_empty(), "{:?}", diagnose(&[menu]));

    // And a mistyped one gets the interface's name rather than nothing: one candidate list, which is the
    // project's declarations and the interface's together.
    let typo = "screen pause:\n    button:\n        text \"Preferences\"\n        action open_screen(\"preferances\")\n";
    let found = diagnostics(&[typo]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].code.as_str(), "E5009");
    assert_eq!(
        found[0].help.as_deref(),
        Some("did you mean `preferences`?")
    );
}

/// A typo in the string spelling gets the nearest screen's name, like every other name in the language.
#[test]
fn an_unrecognisable_target_gets_a_suggestion() {
    let menu = "screen pause:\n    button:\n        text \"Go\"\n        action open_screen(\"settingss\")\n";
    let settings = "screen settings():\n    text \"Settings\"\n";

    let found = diagnostics(&[menu, settings]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].code.as_str(), "E5009");
    assert_eq!(
        found[0].help.as_deref(),
        Some("did you mean `settings`?"),
        "{:?}",
        found[0]
    );
}
