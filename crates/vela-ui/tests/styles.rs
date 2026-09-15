//! Style inheritance and style references.
//!
//! Two codes, and one of them exists to turn a hang into an error: an inheritance loop would
//! otherwise recurse until the stack ended, and the diagnostic would be a crash in whichever
//! tool resolved it first.

use vela_span::FileId;
use vela_syntax::{Item, ScreenDecl, StyleDecl, parse};
use vela_ui::{check_inheritance, check_screen_styles};

/// Every `style` declared in a source.
fn styles_of(source: &str) -> Vec<StyleDecl> {
    let parsed = parse(FileId::from_raw(0), source);
    parsed
        .program
        .items
        .into_iter()
        .filter_map(|item| match item {
            Item::Style(style) => Some(style),
            _ => None,
        })
        .collect()
}

/// The single screen in a source.
fn screen_of(source: &str) -> ScreenDecl {
    let parsed = parse(FileId::from_raw(0), source);
    assert!(parsed.diagnostics.is_empty(), "the fixture must parse");
    parsed
        .program
        .items
        .into_iter()
        .find_map(|item| match item {
            Item::Screen(screen) => Some(screen),
            _ => None,
        })
        .expect("expected a screen")
}

/// Borrowed styles, which is the shape the checks take — a `StyleDecl` holds expressions, so
/// passing them by value would mean cloning a tree to ask a question about names.
fn refs(styles: &[StyleDecl]) -> Vec<&StyleDecl> {
    styles.iter().collect()
}

fn codes(diagnostics: &[vela_diag::Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|d| d.code.as_str().to_string())
        .collect()
}

/// A valid chain is clean — the checker's first job is not to cry wolf.
#[test]
fn a_valid_chain_is_clean() {
    let styles = styles_of(
        "style body:\n    color = 0xffffff\n\nstyle speaker from body:\n    weight = bold\n",
    );
    assert_eq!(styles.len(), 2);
    assert!(check_inheritance(&refs(&styles)).is_empty());
}

/// `E5008` — the shortest loop, reported as itself rather than as a cycle.
#[test]
fn a_style_inheriting_from_itself_is_reported() {
    let styles = styles_of("style a from a:\n    color = 0x0\n");
    let diagnostics = check_inheritance(&refs(&styles));
    assert_eq!(codes(&diagnostics), vec!["E5008"]);
    assert!(diagnostics[0].message.contains("itself"));
    assert!(diagnostics[0].help.is_some(), "and it says what to do");
}

/// `E5008` — a loop of two, which is the case a self-check would miss.
#[test]
fn a_two_style_cycle_is_reported() {
    let styles =
        styles_of("style a from b:\n    color = 0x0\n\nstyle b from a:\n    color = 0x0\n");
    let diagnostics = check_inheritance(&refs(&styles));
    assert_eq!(
        codes(&diagnostics),
        vec!["E5008", "E5008"],
        "both declarations are at fault — fixing one leaves the loop"
    );
    assert!(
        diagnostics[0].message.contains('→') || diagnostics[0].message.contains("->"),
        "the message should show the chain: {}",
        diagnostics[0].message
    );
}

/// `E5007` — a `from` naming nothing, with a suggestion.
#[test]
fn an_unknown_base_get_the_code_and_a_suggestion() {
    let styles =
        styles_of("style body:\n    color = 0x0\n\nstyle speaker from boddy:\n    weight = bold\n");
    let diagnostics = check_inheritance(&refs(&styles));
    assert_eq!(codes(&diagnostics), vec!["E5007"]);
    assert!(
        diagnostics[0]
            .help
            .as_deref()
            .is_some_and(|help| help.contains("body")),
        "{:?}",
        diagnostics[0].help
    );
}

/// `E5007` — a screen's `style` prop naming nothing.
#[test]
fn an_unknown_style_on_a_screen_is_reported() {
    let styles = styles_of("style body:\n    color = 0x0\n");
    let screen = screen_of("screen s:\n    box:\n        text \"hi\" style = boddy\n");
    let diagnostics = check_screen_styles(&screen.body, &refs(&styles));
    assert_eq!(codes(&diagnostics), vec!["E5007"]);
    assert!(diagnostics[0].message.contains("boddy"));
}

/// A screen naming a declared style is clean, including on the far side of a conditional.
#[test]
fn a_declared_style_is_accepted_anywhere_in_the_tree() {
    let styles = styles_of("style body:\n    color = 0x0\n");
    let screen =
        screen_of("screen s:\n    box:\n        if flag:\n            text \"hi\" style = body\n");
    let diagnostics = check_screen_styles(&screen.body, &refs(&styles));
    assert!(diagnostics.is_empty(), "{:?}", codes(&diagnostics));
}

/// A three-step chain that does not loop is clean — the walk has to follow a base it has not
/// yet seen, and stopping early would report a valid chain as a cycle.
#[test]
fn a_long_valid_chain_is_clean() {
    let styles = styles_of(
        "style a:\n    color = 0x0\n\nstyle b from a:\n    weight = bold\n\nstyle c from b:\n    size = 12\n",
    );
    assert!(
        check_inheritance(&refs(&styles)).is_empty(),
        "{:?}",
        codes(&check_inheritance(&refs(&styles)))
    );
}

/// A style with no `from` is not part of any chain and never reports.
#[test]
fn a_root_style_is_clean() {
    let styles = styles_of("style body:\n    color = 0x0\n");
    assert!(check_inheritance(&refs(&styles)).is_empty());
}

/// Every declared style is resolvable by name, which is what the checker's `known` list is
/// built from.
#[test]
fn every_style_resolves() {
    let styles = styles_of("style a:\n    color = 0x0\n\nstyle b:\n    color = 0x1\n");
    for name in vela_ui::styles::names(&refs(&styles)) {
        assert!(vela_ui::resolve(&refs(&styles), name).is_some(), "{name}");
    }
    assert!(vela_ui::resolve(&refs(&styles), "missing").is_none());
}
