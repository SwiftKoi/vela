//! Checking a widget tree against the registry.
//!
//! This is the exit criterion that asks for *"unknown widget / wrong prop / bad arg each
//! produce the right `E5xxx` with a suggestion"* — so every test here asserts a code **and**
//! the help text, because a code without a suggestion is half the diagnostic.

use vela_span::FileId;
use vela_syntax::{Item, parse};
use vela_ui::{WidgetRegistry, check_screen};

/// Checks a screen body and returns `(code, help)` for each diagnostic.
fn diagnose(body: &str) -> Vec<(String, String)> {
    let source = format!("screen s:\n{body}");
    let parsed = parse(FileId::from_raw(0), &source);
    assert!(
        parsed.diagnostics.is_empty(),
        "the fixture must parse: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| d.message.clone())
            .collect::<Vec<_>>()
    );
    let Some(Item::Screen(screen)) = parsed.program.items.first() else {
        panic!("expected a screen");
    };
    check_screen(&screen.body, &WidgetRegistry::builtin())
        .into_iter()
        .map(|d| {
            (
                d.code.as_str().to_string(),
                d.help.unwrap_or_else(|| "<no help>".to_string()),
            )
        })
        .collect()
}

/// A valid screen produces nothing. The checker's first job is not to cry wolf.
#[test]
fn a_valid_screen_is_clean() {
    let source = "screen dialogue(name: str?, line: str):\n    layer ui\n    box at bottom:\n        pad 24\n        column gap 8:\n            text name\n            text line style = body\n";
    let parsed = parse(FileId::from_raw(0), source);
    let Some(Item::Screen(screen)) = parsed.program.items.first() else {
        panic!("expected a screen");
    };
    let diagnostics = check_screen(&screen.body, &WidgetRegistry::builtin());
    assert!(
        diagnostics.is_empty(),
        "{:?}",
        diagnostics
            .iter()
            .map(|d| format!("{}: {}", d.code.as_str(), d.message))
            .collect::<Vec<_>>()
    );
}

/// `E5005`, with the name it probably meant.
#[test]
fn an_unknown_widget_is_reported_with_a_suggestion() {
    let diagnostics = diagnose("    colunm\n");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    let (code, help) = &diagnostics[0];
    assert_eq!(code, "E5005");
    assert!(help.contains("column"), "{help}");
}

/// A name nowhere near anything registered gets the code and no guess.
///
/// A wrong suggestion is worse than none: it sends someone looking in the wrong place.
#[test]
fn an_unrecognisable_widget_gets_no_suggestion() {
    let diagnostics = diagnose("    canvas\n");
    assert_eq!(diagnostics[0].0, "E5005");
    assert_eq!(diagnostics[0].1, "<no help>");
}

/// `E5006` — a prop that exists, just not on this widget.
#[test]
fn a_prop_on_the_wrong_widget_is_reported() {
    let diagnostics = diagnose("    text \"hi\" columns 3\n");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].0, "E5006");
}

/// A misspelled prop suggests the one that was meant.
#[test]
fn a_misspelled_prop_suggests_the_right_one() {
    let diagnostics = diagnose("    row gapp 8:\n        text \"hi\"\n");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    let (code, help) = &diagnostics[0];
    assert_eq!(code, "E5006");
    assert!(help.contains("gap"), "{help}");
}

/// `E5004` — a single-child widget given two.
///
/// The most common screen mistake after a typo: indenting a sibling one level too far.
#[test]
fn two_children_under_a_single_child_widget_is_reported() {
    let diagnostics = diagnose("    box:\n        text \"a\"\n        text \"b\"\n");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].0, "E5004");
    assert!(
        diagnostics[0].1.contains("column"),
        "the help should say what to do"
    );
}

/// A prop on its own line belongs to the widget above it, and is not an unknown widget.
///
/// This is the ambiguity the parser cannot resolve and the registry can: `pad` is not a
/// widget, and `box` takes it.
#[test]
fn a_bare_prop_line_is_not_an_unknown_widget() {
    let diagnostics = diagnose("    box:\n        pad 24\n        text \"hi\"\n");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

/// A prop line under a widget that does *not* take it is still a mistake — and the mistake is
/// an unknown widget, because that is what it looks like from here.
#[test]
fn a_prop_line_under_the_wrong_parent_is_reported() {
    let diagnostics = diagnose("    text \"hi\"\n    pad 24\n");
    assert!(!diagnostics.is_empty(), "a stray prop should be reported");
}

/// One mistake does not hide the next: both unknown widgets are reported.
#[test]
fn every_unknown_widget_is_reported() {
    let diagnostics = diagnose("    colunm\n    txt \"a\"\n    roww\n");
    assert_eq!(diagnostics.len(), 3, "{diagnostics:?}");
}

/// A `Layer` line is not a widget and is not checked against the registry.
#[test]
fn a_layer_line_is_not_a_widget() {
    let diagnostics = diagnose("    layer ui\n    box:\n        text \"hi\"\n");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

/// An `if` inside a widget keeps that widget as its parent, so props inside it still resolve.
#[test]
fn a_conditional_keeps_its_parent_widget() {
    let diagnostics = diagnose("    box:\n        if true:\n            pad 24\n    \n");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}
