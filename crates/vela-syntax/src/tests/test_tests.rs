//! Parsing a `test` item.
//!
//! The shape is pinned here and the *canonical form* by `tests/golden/parse/item_test.vela`, which the
//! repository-wide corpus test formats and re-parses. What this covers that a golden cannot is the
//! errors: an unknown directive is one diagnostic naming the line, not a line that quietly does nothing.

use crate::tree::{CoverMode, DirectiveKind, Item, TestDecl};

use super::parse_tests::parse_src;

/// The one test a source declares.
fn only_test(src: &str) -> TestDecl {
    let result = parse_src(src);
    assert!(
        result.diagnostics.is_empty(),
        "expected a clean parse, got {:?}",
        result.diagnostics
    );

    match result.program.items.into_iter().next() {
        Some(Item::Test(decl)) => decl,
        other => panic!("expected a test, got {other:?}"),
    }
}

/// Every directive, and the name that holds them together.
#[test]
fn a_test_parses_its_name_and_every_directive() {
    let test = only_test(
        "test \"picking the forest sets trust\":\n\
         \x20   run from chapters.forest.clearing\n\
         \x20   choose \"Explore\"\n\
         \x20   click \"Settings\"\n\
         \x20   advance 4\n\
         \x20   expect trust == 1\n\
         \x20   cover variants\n",
    );

    assert_eq!(test.name, "picking the forest sets trust");
    assert_eq!(test.directives.len(), 6);

    match &test.directives[0].kind {
        DirectiveKind::Run { target, .. } => {
            assert_eq!(
                target.join("."),
                "chapters.forest.clearing",
                "a dotted path"
            );
        }
        other => panic!("expected a `run`, got {other:?}"),
    }
    assert!(matches!(
        test.directives[1].kind,
        DirectiveKind::Choose { .. }
    ));
    assert!(matches!(
        test.directives[2].kind,
        DirectiveKind::Click { .. }
    ));
    match &test.directives[3].kind {
        DirectiveKind::Advance { count } => assert_eq!(*count, 4),
        other => panic!("expected an `advance`, got {other:?}"),
    }
    assert!(matches!(
        test.directives[4].kind,
        DirectiveKind::Expect { .. }
    ));
    match &test.directives[5].kind {
        DirectiveKind::Cover { mode } => assert_eq!(*mode, CoverMode::Variants),
        other => panic!("expected a `cover`, got {other:?}"),
    }
}

/// `run` without `from` starts where the game does, which is the entry label rather than a gap.
#[test]
fn a_bare_run_names_no_label() {
    let test = only_test("test \"from the top\":\n    run\n");

    match &test.directives[0].kind {
        DirectiveKind::Run { target, .. } => {
            assert!(target.is_empty(), "the entry label: {target:?}");
        }
        other => panic!("expected a `run`, got {other:?}"),
    }
}

/// The directives are contextual, so a story keeps `run`, `advance`, `choose`, `click`, `expect`, and
/// `cover` as names. `expect` is the interesting one: it is a perfectly ordinary label name, and
/// reserving it would have taken it away from every story in the repository.
#[test]
fn the_directive_names_are_still_ordinary_names() {
    let result = parse_src(
        "label expect:\n    jump cover\n\nfn run(advance: int) -> int:\n    return advance\n",
    );

    assert!(
        result.diagnostics.is_empty(),
        "the words are names outside a test: {:?}",
        result.diagnostics
    );
}

/// An unknown directive is reported once, with the line it is on.
#[test]
fn an_unknown_directive_is_reported() {
    let result = parse_src("test \"typo\":\n    ecpect trust == 1\n");

    let codes: Vec<&str> = result
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect();
    assert_eq!(codes, vec!["E1001"], "{:?}", result.diagnostics);

    let message = &result.diagnostics[0].message;
    assert!(
        message.contains("run") && message.contains("cover"),
        "the message lists what is allowed: {message}"
    );
}

/// `cover` takes one of two words, and says so when it is given another.
#[test]
fn an_unknown_cover_mode_is_reported() {
    let result = parse_src("test \"no\":\n    cover everything\n");

    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    assert!(
        result.diagnostics[0]
            .message
            .contains("`labels` or `variants`"),
        "{:?}",
        result.diagnostics[0]
    );
}
