//! The accessibility tree and the label lint.
//!
//! `SCREENS.md §10` says accessibility is *structural, not a mode*. That means the tree is
//! derived from a screen rather than declared alongside it, so the tests are about the
//! derivation: that roles come from widgets, that labels come from content, and that focus
//! order is tree order with nothing skipped.

use vela_span::FileId;
use vela_syntax::{Item, ScreenDecl, ScreenLine, parse};
use vela_ui::{Role, WidgetRegistry, a11y::check_labels, a11y::tree};

/// No screens to compose with.
///
/// Every fixture here is one screen with no `use`, so the question `tree` asks of the other
/// declarations has no answer to give. Composition is exercised where it belongs, in
/// `tests/compose.rs`.
const NO_SCREENS: &[&ScreenDecl] = &[];

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

fn codes(diagnostics: &[vela_diag::Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|d| d.code.as_str().to_string())
        .collect()
}

/// Roles come from the widget, so a screen cannot get them wrong.
#[test]
fn roles_come_from_the_widget() {
    assert_eq!(
        Role::of("button", vela_ui::Category::Interactive),
        Role::Button
    );
    assert_eq!(
        Role::of("bar", vela_ui::Category::Interactive),
        Role::Slider
    );
    assert_eq!(Role::of("text", vela_ui::Category::Leaf), Role::Text);
    assert_eq!(
        Role::of("column", vela_ui::Category::Container),
        Role::Group
    );
    // A plugin's interactive widget is still a button to a screen reader, even though its name
    // is one we have never seen.
    assert_eq!(
        Role::of("carousel", vela_ui::Category::Interactive),
        Role::Button
    );
}

/// A button whose content is the words is named by those words — no `label` needed.
#[test]
fn a_button_is_named_by_its_text_child() {
    let body = body_of("screen s:\n    button:\n        text \"Tell the truth\"\n");
    let nodes = tree(&body, &WidgetRegistry::builtin(), NO_SCREENS);
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].role, Role::Button);
    assert_eq!(nodes[0].label.as_deref(), Some("Tell the truth"));
}

/// An explicit label wins over the content, because a button reading "OK" may need to say more.
#[test]
fn an_explicit_label_wins() {
    let body =
        body_of("screen s:\n    button label \"Confirm the choice\":\n        text \"OK\"\n");
    let nodes = tree(&body, &WidgetRegistry::builtin(), NO_SCREENS);
    assert_eq!(nodes[0].label.as_deref(), Some("Confirm the choice"));
}

/// `W4010` — an interactive widget with nothing to announce.
#[test]
fn an_unlabelled_button_is_reported() {
    let body = body_of("screen s:\n    button:\n        image \"icon.arrow\"\n");
    let diagnostics = check_labels(&body, &WidgetRegistry::builtin());
    assert_eq!(
        codes(&diagnostics),
        vec!["W4010"],
        "{:?}",
        codes(&diagnostics)
    );
    assert!(diagnostics[0].help.is_some(), "and it says what to do");
}

/// A non-interactive widget is not expected to be labelled: a column has nothing to announce.
#[test]
fn a_container_is_not_linted() {
    let body = body_of("screen s:\n    column:\n        text \"hi\"\n");
    assert!(check_labels(&body, &WidgetRegistry::builtin()).is_empty());
}

/// An interpolation is not a label. A screen reader needs something it can announce before the
/// story has run, and `[name]` is not that.
#[test]
fn an_interpolated_text_is_not_a_label() {
    let body = body_of("screen s:\n    button:\n        text \"[name]\"\n");
    let nodes = tree(&body, &WidgetRegistry::builtin(), NO_SCREENS);
    assert_eq!(nodes[0].label, None);
    assert_eq!(
        codes(&check_labels(&body, &WidgetRegistry::builtin())),
        vec!["W4010"]
    );
}

/// Focus order is tree order, and only focusable things get an index.
///
/// Counting containers too would make "tab three times" land somewhere surprising.
#[test]
fn focus_order_is_tree_order_and_skips_containers() {
    let body = body_of(
        "screen s:\n    column:\n        button:\n            text \"One\"\n        text \"A label\"\n        button:\n            text \"Two\"\n",
    );
    let nodes = tree(&body, &WidgetRegistry::builtin(), NO_SCREENS);
    assert_eq!(nodes.len(), 1, "the column is the only top-level node");
    let column = &nodes[0];
    assert_eq!(column.focus, None, "a column is not focusable");
    let children = &column.children;
    assert_eq!(children[0].focus, Some(0), "the first button");
    assert_eq!(children[1].focus, None, "static text is not focusable");
    assert_eq!(children[2].focus, Some(1), "the second button");
}

/// A conditional contributes its branches' nodes: what a screen reader reads is what is on
/// screen, and which branch that is is a runtime question.
#[test]
fn a_conditional_contributes_its_branches() {
    let body = body_of(
        "screen s:\n    column:\n        if flag:\n            button:\n                text \"Shown\"\n",
    );
    let nodes = tree(&body, &WidgetRegistry::builtin(), NO_SCREENS);
    let column = &nodes[0];
    assert_eq!(column.children.len(), 1);
    assert_eq!(column.children[0].role, Role::Button);
    assert_eq!(column.children[0].focus, Some(0));
}

/// An empty screen has an empty tree rather than a panic.
#[test]
fn an_empty_screen_has_no_nodes() {
    assert!(
        tree(
            &body_of("screen s:\n    pass\n"),
            &WidgetRegistry::builtin(),
            NO_SCREENS,
        )
        .is_empty()
    );
}

/// The lint walks into containers, so a nested button is still found.
#[test]
fn a_nested_unlabelled_button_is_found() {
    let body = body_of("screen s:\n    column:\n        row:\n            button label \"ok\"\n");
    assert!(check_labels(&body, &WidgetRegistry::builtin()).is_empty());
}
