//! String literals: the two sigils, what an escape means, and where each part is.
//!
//! Split from `parse_tests` because it is a phase of its own — the contents of a string are scanned
//! and re-lexed rather than parsed as tokens — and because the rules here are the ones a reader of
//! the language is most likely to hit.

use crate::tree::{Expr, Stmt, StrPart};

use super::parse_tests::{only_label, parse_src};

#[test]
fn a_string_splits_into_literal_and_interpolated_parts() {
    let body = only_label("label a:\n    var s = \"a [score] b\"\n");
    let Stmt::Var(var) = &body[0] else {
        panic!("expected a var statement");
    };
    let Expr::Str { parts, .. } = &var.value else {
        panic!("expected a string, got {:?}", var.value);
    };
    assert_eq!(parts.len(), 3);
    assert!(matches!(&parts[0], StrPart::Literal { text, .. } if text == "a "));
    assert!(matches!(&parts[1], StrPart::Interpolation { .. }));
    assert!(matches!(&parts[2], StrPart::Literal { text, .. } if text == " b"));
}

#[test]
fn an_interpolated_expression_keeps_absolute_spans() {
    let src = "label a:\n    var s = \"a [score] b\"\n";
    let body = only_label(src);
    let Stmt::Var(var) = &body[0] else {
        panic!("expected a var statement");
    };
    let Expr::Str { parts, .. } = &var.value else {
        panic!("expected a string");
    };
    let StrPart::Interpolation { expr, .. } = &parts[1] else {
        panic!("expected an interpolation");
    };

    // The inner expression must point at `score` in the real file, not in the extracted fragment, or
    // every diagnostic inside `[...]` would point at nonsense.
    let span = expr.span();
    let text = &src[span.start() as usize..span.end() as usize];
    assert_eq!(text, "score");
}

/// A nested bracket belongs to the body, not to the string: a scanner that stopped at the first `]`
/// would cut `[list[0]]` in half and re-lex the rest as source.
#[test]
fn an_interpolated_body_may_contain_brackets() {
    let body = only_label("label a:\n    var s = \"at [list[0]]\"\n");
    let Stmt::Var(var) = &body[0] else {
        panic!("expected a var statement");
    };
    let Expr::Str { parts, .. } = &var.value else {
        panic!("expected a string");
    };

    assert_eq!(parts.len(), 2);
    assert!(matches!(&parts[1], StrPart::Interpolation { .. }));
}

#[test]
fn a_doubled_sigil_is_literal_text() {
    let src = "label a:\n    var s = \"a [[ bracket and a {{ brace\"\n";
    let body = only_label(src);
    let Stmt::Var(var) = &body[0] else {
        panic!("expected a var statement");
    };
    let Expr::Str { parts, .. } = &var.value else {
        panic!("expected a string");
    };
    assert_eq!(parts.len(), 1);
    let StrPart::Literal { span, text } = &parts[0] else {
        panic!("expected a literal");
    };
    assert_eq!(text, "a [ bracket and a { brace");

    // The span covers what was *written*, not what the text became: a doubled sigil is two bytes of
    // source and one byte of text, and a span computed from the text length would be short.
    assert_eq!(
        &src[span.start() as usize..span.end() as usize],
        "a [[ bracket and a {{ brace"
    );
}

/// A literal part that ends at an interpolation is measured from where the part began, which is not
/// the same as counting back from the `[` once an escape is in it.
#[test]
fn a_literal_before_an_interpolation_is_spanned_as_written() {
    let src = "label a:\n    var s = \"a [[ b [score]\"\n";
    let body = only_label(src);
    let Stmt::Var(var) = &body[0] else {
        panic!("expected a var statement");
    };
    let Expr::Str { parts, .. } = &var.value else {
        panic!("expected a string");
    };
    let StrPart::Literal { span, text } = &parts[0] else {
        panic!("expected a literal");
    };

    assert_eq!(text, "a [ b ");
    assert_eq!(&src[span.start() as usize..span.end() as usize], "a [[ b ");
}

/// A backslash before a sigil is the older spelling of the same thing, and is read as one: it must
/// not begin a tag or an interpolation.
#[test]
fn an_escaped_sigil_is_literal_text() {
    let body = only_label("label a:\n    var s = \"a \\[ bracket and a \\{ brace\"\n");
    let Stmt::Var(var) = &body[0] else {
        panic!("expected a var statement");
    };
    let Expr::Str { parts, .. } = &var.value else {
        panic!("expected a string");
    };
    assert_eq!(parts.len(), 1);
    assert!(
        matches!(&parts[0], StrPart::Literal { text, .. } if text == "a [ bracket and a { brace")
    );
}

/// A text tag is part of the text rather than a part of its own.
///
/// `BYTECODE.md §3.3`: a line of dialogue is one command, and what a script puts inside that text
/// is interpreted by the presenter. So the tag's characters stay in the literal, which is what
/// makes a migrated line byte-identical to the one its author wrote.
#[test]
fn a_known_text_tag_stays_in_the_text() {
    let body = only_label("label a:\n    var s = \"a {b}bold{/b} b\"\n");
    let Stmt::Var(var) = &body[0] else {
        panic!("expected a var statement");
    };
    let Expr::Str { parts, .. } = &var.value else {
        panic!("expected a string");
    };
    assert_eq!(parts.len(), 1);
    assert!(
        matches!(&parts[0], StrPart::Literal { text, .. } if text == "a {b}bold{/b} b"),
        "{parts:?}"
    );
}

/// A tag outside the vocabulary is an error rather than text, because the two readings differ in
/// meaning: Ren'Py's `{color=#fff}` styles the words around it, and drawing the markup literally
/// would put it in front of a player.
#[test]
fn an_unknown_text_tag_is_reported() {
    let parsed = parse_src("label a:\n    var s = \"a {color=#fff} b\"\n");
    let codes: Vec<&str> = parsed
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect();

    assert_eq!(codes, vec!["E0010"]);
}
