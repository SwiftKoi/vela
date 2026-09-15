//! Hot reload: what survives an edit.
//!
//! `SCREENS.md §12`. Rebuild is *always* safe — a screen is a pure function of its arguments
//! and bound state, so there is no hidden mutation to lose. The question these tests ask is
//! narrower and more interesting: **what is worth carrying across**, which is a question about
//! identity.

use vela_span::FileId;
use vela_syntax::{Item, ScreenLine, parse};
use vela_ui::{Key, diff};

fn body_of(source: &str) -> Vec<ScreenLine> {
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
        .program
        .items
        .into_iter()
        .find_map(|item| match item {
            Item::Screen(screen) => Some(screen.body),
            _ => None,
        })
        .expect("expected a screen")
}

/// A diff between two screen bodies.
fn between(before: &str, after: &str) -> vela_ui::Diff {
    diff(&body_of(before), &body_of(after))
}

/// Editing a prop leaves every node where it was, and every node keeps its state.
///
/// This is the ordinary case: an author nudges a padding or changes a colour, and the input
/// box they were typing in keeps its cursor.
#[test]
fn editing_a_prop_keeps_everything() {
    let result = between(
        "screen s:\n    box:\n        text \"hi\"\n",
        "screen s:\n    box:\n        text \"hello\"\n",
    );
    assert!(!result.rebuild, "{:?}", result.note);
    assert_eq!(result.kept_count(), 2, "the box and the text");
    assert!(result.keeps(&Key::Path(vec![0])));
    assert!(result.keeps(&Key::Path(vec![0, 0])));
}

/// An `id` survives being moved, which a position cannot.
#[test]
fn an_id_survives_a_move() {
    let before =
        "screen s:\n    column:\n        text \"a\" id first\n        text \"b\" id second\n";
    let after =
        "screen s:\n    column:\n        text \"b\" id second\n        text \"a\" id first\n";
    let result = between(before, after);

    assert!(!result.rebuild);
    assert!(result.keeps(&Key::Id("first".to_string())));
    assert!(result.keeps(&Key::Id("second".to_string())));
}

/// Positional state does *not* survive a move — and that is the reason `id` exists.
#[test]
fn a_position_does_not_survive_a_move() {
    let before = "screen s:\n    column:\n        bar\n";
    // A node removed from the front moves everything after it.
    let after = "screen s:\n    column:\n        text \"new\"\n        bar\n";
    let result = between(before, after);

    assert!(!result.rebuild);
    assert!(
        !result.keeps(&Key::Path(vec![0, 1])),
        "the bar moved, so its old position's state belongs to the node that left"
    );
}

/// Replacing the widget at a position is a different node, whatever its position says.
#[test]
fn a_different_widget_at_a_position_is_a_different_node() {
    let before = "screen s:\n    box:\n        bar\n";
    let after = "screen s:\n    box:\n        text \"x\"\n";
    let result = between(before, after);
    assert!(
        !result.keeps(&Key::Path(vec![0, 0])),
        "a bar's state is not a text's state"
    );
}

/// Changing the root is a different screen, and §12 says so with a note.
#[test]
fn a_changed_root_rebuilds_with_a_note() {
    let result = between(
        "screen s:\n    box:\n        text \"a\"\n",
        "screen s:\n    column:\n        text \"a\"\n",
    );
    assert!(result.rebuild);
    assert!(result.note.is_some(), "the console needs a reason");
    assert!(result.kept.is_empty());
}

/// Two nodes claiming one id cannot be matched, and guessing would be worse than losing both:
/// it would move one widget's scroll position to another and look like it worked.
#[test]
fn a_duplicate_id_rebuilds_rather_than_guessing() {
    let result = between(
        "screen s:\n    column:\n        text \"a\" id x\n",
        "screen s:\n    column:\n        text \"a\" id x\n        text \"b\" id x\n",
    );
    assert!(result.rebuild);
    let note = result.note.expect("a note");
    assert!(note.contains('x'), "{note}");
    assert!(note.contains('2'), "the count helps: {note}");
}

/// An insertion before an identified node does not disturb it; the same insertion before an
/// unidentified one does. That pair is the whole argument for `id`.
#[test]
fn an_insertion_before_an_id_is_survivable() {
    let result = between(
        "screen s:\n    column:\n        text \"a\" id body\n",
        "screen s:\n    column:\n        text \"hint\"\n        text \"a\" id body\n",
    );
    assert!(!result.rebuild);
    assert!(result.keeps(&Key::Id("body".to_string())));
    assert!(
        !result.keeps(&Key::Path(vec![0, 0])),
        "and the new node has no old state to have kept"
    );
}

/// Two identical screens keep everything, which is what a save that changed nothing looks
/// like.
#[test]
fn an_unchanged_screen_keeps_everything() {
    let source = "screen s:\n    box:\n        column gap 8:\n            text \"a\"\n";
    let result = between(source, source);
    assert!(!result.rebuild);
    assert_eq!(result.kept_count(), 3, "box, column, text");
}

/// A conditional contributes its children where they are, rather than under a path nobody can
/// see.
#[test]
fn a_conditional_does_not_add_a_level() {
    let result = between(
        "screen s:\n    box:\n        if flag:\n            text \"a\"\n",
        "screen s:\n    box:\n        if flag:\n            text \"b\"\n",
    );
    assert!(!result.rebuild);
    assert!(result.keeps(&Key::Path(vec![0, 0])), "{:?}", result.kept);
}
