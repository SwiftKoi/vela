use vela_span::FileId;

use crate::{Keyword, TokenKind, lex};

/// Lexes `src` and returns the token kinds plus the diagnostic codes.
fn lex_src(src: &str) -> (Vec<TokenKind>, Vec<String>) {
    let result = lex(FileId::from_raw(0), src);
    let kinds = result.tokens.iter().map(|token| token.kind).collect();
    let codes = result
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str().to_string())
        .collect();
    (kinds, codes)
}

fn kinds(src: &str) -> Vec<TokenKind> {
    lex_src(src).0
}

fn codes(src: &str) -> Vec<String> {
    lex_src(src).1
}

fn kw(keyword: Keyword) -> TokenKind {
    TokenKind::Keyword(keyword)
}

#[test]
fn lexes_a_label_and_its_block() {
    use TokenKind::{Colon, Dedent, Ident, Indent, Newline, Str};

    assert_eq!(
        kinds("label start:\n    \"hi\"\n"),
        vec![
            kw(Keyword::Label),
            Ident,
            Colon,
            Newline,
            Indent,
            Str,
            Newline,
            Dedent,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn blank_and_comment_lines_do_not_open_a_block() {
    let src = "label a:\n\n    # a comment\n    \"x\"\n";
    let (kinds, codes) = lex_src(src);

    assert!(codes.is_empty(), "{codes:?}");
    // Exactly one indent: the blank line and the comment line contributed nothing, so
    // neither could have closed or re-opened the block.
    assert_eq!(
        kinds
            .iter()
            .filter(|kind| **kind == TokenKind::Indent)
            .count(),
        1
    );
    assert_eq!(
        kinds
            .iter()
            .filter(|kind| **kind == TokenKind::Dedent)
            .count(),
        1
    );
}

#[test]
fn nested_blocks_emit_nested_indents() {
    let src = "label a:\n    if x:\n        \"y\"\n";
    let kinds = kinds(src);

    assert_eq!(
        kinds
            .iter()
            .filter(|kind| **kind == TokenKind::Indent)
            .count(),
        2
    );
    assert_eq!(
        kinds
            .iter()
            .filter(|kind| **kind == TokenKind::Dedent)
            .count(),
        2
    );
    // The file ends with a dedent for every block still open.
    assert_eq!(kinds.last(), Some(&TokenKind::Eof));
    assert_eq!(kinds[kinds.len() - 2], TokenKind::Dedent);
}

#[test]
fn a_dedent_can_return_to_an_outer_block() {
    let src = "label a:\n    if x:\n        \"y\"\n    \"z\"\n";
    let (kinds, codes) = lex_src(src);
    assert!(codes.is_empty(), "{codes:?}");

    // One indent for each block, then one dedent mid-file and one at EOF.
    assert_eq!(
        kinds
            .iter()
            .filter(|kind| **kind == TokenKind::Dedent)
            .count(),
        2
    );
}

#[test]
fn keywords_are_distinguished_from_identifiers() {
    let kinds = kinds("label labelled jump jumper\n");
    assert_eq!(
        kinds,
        vec![
            kw(Keyword::Label),
            TokenKind::Ident,
            kw(Keyword::Jump),
            TokenKind::Ident,
            TokenKind::Newline,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn the_keyword_table_is_sorted_so_the_lookup_is_correct() {
    let mut previous = "";
    for (_, spelling) in Keyword::ALL {
        assert!(
            *spelling > previous,
            "`{spelling}` should sort after `{previous}`; from_str binary-searches this table"
        );
        previous = spelling;
    }
}

#[test]
fn every_keyword_maps_to_itself() {
    for (keyword, spelling) in Keyword::ALL {
        assert_eq!(Keyword::lookup(spelling), Some(*keyword));
        assert_eq!(keyword.as_str(), *spelling);
    }
}

#[test]
fn operators_include_the_two_byte_forms() {
    let kinds = kinds("a == b != c <= d >= e -> f => g ?? h\n");
    for expected in [
        TokenKind::EqEq,
        TokenKind::BangEq,
        TokenKind::Le,
        TokenKind::Ge,
        TokenKind::Arrow,
        TokenKind::FatArrow,
        TokenKind::QuestionQuestion,
    ] {
        assert!(kinds.contains(&expected), "missing {expected:?}");
    }
}

#[test]
fn augmented_assignment_operators_are_single_tokens() {
    let kinds = kinds("a += b -= c *= d /= e\n");
    for expected in [
        TokenKind::PlusEq,
        TokenKind::MinusEq,
        TokenKind::StarEq,
        TokenKind::SlashEq,
    ] {
        assert!(kinds.contains(&expected), "missing {expected:?}");
    }
}

#[test]
fn numeric_literals_are_classified() {
    let cases = [
        ("1", TokenKind::Int),
        ("1_000", TokenKind::Int),
        ("1.5", TokenKind::Float),
        ("1_0.5", TokenKind::Float),
        ("1e3", TokenKind::Float),
        ("1E3", TokenKind::Float),
        ("1.5e-2", TokenKind::Float),
    ];
    for (literal, expected) in cases {
        let src = format!("label a:\n    var x = {literal}\n");
        let (kinds, codes) = lex_src(&src);
        assert!(codes.is_empty(), "{literal}: {codes:?}");
        assert!(
            kinds.contains(&expected),
            "{literal} should lex as {expected:?}"
        );
    }
}

#[test]
fn a_trailing_e_is_an_identifier_not_a_broken_exponent() {
    let (kinds, codes) = lex_src("label a:\n    var x = 1e\n");
    assert!(codes.is_empty(), "{codes:?}");
    assert!(kinds.contains(&TokenKind::Int));
    assert!(kinds.contains(&TokenKind::Ident));
}

#[test]
fn a_dot_after_an_integer_is_field_access_not_a_float() {
    let (kinds, codes) = lex_src("label a:\n    var x = 1.foo\n");
    assert!(codes.is_empty(), "{codes:?}");
    assert!(kinds.contains(&TokenKind::Int), "{kinds:?}");
    assert!(kinds.contains(&TokenKind::Dot), "{kinds:?}");
    assert!(kinds.contains(&TokenKind::Ident), "{kinds:?}");
    assert!(!kinds.contains(&TokenKind::Float), "{kinds:?}");
}

#[test]
fn string_literals_keep_their_quotes_and_escapes() {
    let src = "label a:\n    \"he said \\\"hi\\\" \"\n";
    let (kinds, codes) = lex_src(src);
    assert!(codes.is_empty(), "{codes:?}");
    assert_eq!(kinds.iter().filter(|k| **k == TokenKind::Str).count(), 1);
}

#[test]
fn path_literals_are_a_single_token() {
    let (kinds, codes) = lex_src("label a:\n    image bg = @\"art/forest.png\"\n");
    assert!(codes.is_empty(), "{codes:?}");
    assert!(kinds.contains(&TokenKind::Path));
    // The path is one token, not `@` plus a string.
    assert!(!kinds.contains(&TokenKind::Str));
}

#[test]
fn brackets_suppress_statement_breaks() {
    let src = "label a:\n    var x = f(\n        1,\n        2,\n    )\n";
    let kinds = kinds(src);

    let open = kinds.iter().position(|k| *k == TokenKind::LParen).unwrap();
    let close = kinds.iter().position(|k| *k == TokenKind::RParen).unwrap();
    assert!(
        !kinds[open..close].contains(&TokenKind::Newline),
        "a newline inside brackets is a continuation, not a statement break"
    );
}

#[test]
fn a_backslash_continues_a_line() {
    let src = "label a:\n    var x = 1 + \\\n        2\n";
    let kinds = kinds(src);
    // One statement, so one `Newline` before the final dedent.
    assert_eq!(
        kinds.iter().filter(|k| **k == TokenKind::Newline).count(),
        2
    );
}

#[test]
fn crlf_line_endings_are_normalised() {
    let (kinds, codes) = lex_src("label a:\r\n    \"x\"\r\n");
    assert!(codes.is_empty(), "{codes:?}");
    assert!(kinds.contains(&TokenKind::Indent));
    assert!(kinds.contains(&TokenKind::Str));
}

// --- lexical diagnostics -------------------------------------------------------

#[test]
fn e0001_rejects_a_byte_order_mark() {
    assert_eq!(codes("\u{feff}label a:\n"), vec!["E0001"]);
}

#[test]
fn e0002_reports_a_carriage_return_without_a_line_feed() {
    let src = "label a:\n    \"x\"\rlabel b:\n";
    assert_eq!(codes(src), vec!["E0002"]);
}

#[test]
fn e0003_reports_a_tab_used_for_indentation() {
    assert_eq!(codes("label a:\n\t\"x\"\n"), vec!["E0003"]);
}

#[test]
fn e0004_reports_an_inconsistent_indentation_step() {
    // The root's first child indents by four; the second block indents by eight.
    let src = "label a:\n    \"x\"\nlabel b:\n        \"y\"\n";
    assert_eq!(codes(src), vec!["E0004"]);
}

#[test]
fn e0005_reports_a_dedent_that_matches_no_enclosing_block() {
    let src = "label a:\n    if x:\n        \"y\"\n      \"z\"\n";
    assert_eq!(codes(src), vec!["E0005"]);
}

#[test]
fn e0006_reports_a_numeric_literal_that_does_not_fit() {
    assert_eq!(
        codes("label a:\n    var x = 9223372036854775808\n"),
        vec!["E0006"]
    );
}

#[test]
fn e0007_reports_a_trailing_decimal_point() {
    assert_eq!(codes("label a:\n    var x = 1.\n"), vec!["E0007"]);
}

#[test]
fn e0008_reports_an_unterminated_string() {
    assert_eq!(codes("label a:\n    \"unterminated\n"), vec!["E0008"]);
}

#[test]
fn e0009_reports_an_unrecognised_character() {
    assert_eq!(codes("label a:\n    $x\n"), vec!["E0009"]);
}

#[test]
fn recovery_after_a_bad_character_keeps_lexing() {
    let (kinds, codes) = lex_src("label a:\n    $x\n    \"after\"\n");
    assert_eq!(codes, vec!["E0009"]);
    // The rest of the file still lexes, so one stray byte is one diagnostic.
    assert!(kinds.contains(&TokenKind::Str), "{kinds:?}");
    assert!(kinds.contains(&TokenKind::Eof));
}

#[test]
fn a_missing_quote_does_not_swallow_the_rest_of_the_file() {
    let (kinds, codes) = lex_src("label a:\n    \"oops\n    \"fine\"\n");
    assert_eq!(codes, vec!["E0008"]);
    assert!(kinds.contains(&TokenKind::Eof));
}

#[test]
fn a_clean_file_produces_no_diagnostics() {
    let src = "character eileen:\n    name = \"Eileen\"\n\nlabel start:\n    \"Hello, world.\"\n    return\n";
    let (_, codes) = lex_src(src);
    assert!(codes.is_empty(), "{codes:?}");
}

#[test]
fn hexadecimal_literals_are_integers() {
    // Colours are written this way: the obvious `#rrggbb` would collide with comments.
    let src = "label a:\n    var c = 0xff_00_00\n";
    let result = lex(FileId::from_raw(0), src);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);

    let literal = result
        .tokens
        .iter()
        .find(|token| token.kind == TokenKind::Int)
        .expect("an integer token");
    // The whole literal is one token: it was not split into `0` and an identifier.
    let text = &src[literal.span.start() as usize..literal.span.end() as usize];
    assert_eq!(text, "0xff_00_00");
}

#[test]
fn e0006_reports_a_hex_prefix_with_no_digits() {
    assert_eq!(codes("label a:\n    var c = 0x\n"), vec!["E0006"]);
}
