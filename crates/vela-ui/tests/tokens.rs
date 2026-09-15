//! The magic-colour lint, `W4008`.
//!
//! `SCREENS.md §5`: *"only tokens and typed values are permitted; a raw magic color in a
//! screen is `W4008`."* The reason is not tidiness — a literal colour cannot follow a theme,
//! and it sits outside `W4009`'s contrast checking as well.

use vela_span::FileId;
use vela_syntax::{Item, ScreenLine, parse};
use vela_ui::{check_magic_colours, tokens::is_colour_prop};

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

/// A token is what should be written, and it is not reported.
#[test]
fn a_theme_token_is_clean() {
    let body = body_of("screen s:\n    box:\n        text \"hi\" color = theme.accent\n");
    assert!(check_magic_colours(&body).is_empty());
}

/// A literal where a token belongs is reported, with the value and the fix.
#[test]
fn a_literal_colour_is_reported() {
    let body = body_of("screen s:\n    box:\n        text \"hi\" color = 0x112233\n");
    let diagnostics = check_magic_colours(&body);
    assert_eq!(codes(&diagnostics), vec!["W4008"]);

    let message = &diagnostics[0].message;
    assert!(
        message.contains("112233"),
        "the colour should be named: {message}"
    );
    assert!(
        diagnostics[0]
            .help
            .as_deref()
            .is_some_and(|help| help.contains("theme.")),
        "and the fix should be shown"
    );
}

/// The suffix rule catches the props a widget gains later without the lint being told.
#[test]
fn the_colour_prop_rule_is_a_suffix() {
    assert!(is_colour_prop("color"));
    assert!(is_colour_prop("tint"));
    assert!(is_colour_prop("background_color"));
    assert!(!is_colour_prop("grow"));
    assert!(!is_colour_prop("gap"));
}

/// A number that is not a colour is not reported, which is the whole reason the rule is about
/// the prop's name: the parser does not record how a number was written.
#[test]
fn a_plain_number_is_not_a_colour() {
    let body = body_of("screen s:\n    row gap 8:\n        text \"hi\"\n");
    assert!(check_magic_colours(&body).is_empty());
}

/// The lint walks into children and conditionals.
#[test]
fn nested_colours_are_found() {
    let body =
        body_of("screen s:\n    column:\n        if flag:\n            text \"hi\" color = 0x0\n");
    assert_eq!(codes(&check_magic_colours(&body)), vec!["W4008"]);
}

/// Every literal is reported, not just the first: a screen with three magic colours has three
/// things to fix.
#[test]
fn every_literal_colour_is_reported() {
    let body = body_of(
        "screen s:\n    column:\n        text \"a\" color = 0x1\n        text \"b\" tint = 0x2\n",
    );
    assert_eq!(check_magic_colours(&body).len(), 2);
}

/// A screen with no colours at all is clean.
#[test]
fn a_plain_screen_is_clean() {
    let body = body_of("screen s:\n    box:\n        text \"Hello.\"\n");
    assert!(check_magic_colours(&body).is_empty());
}
