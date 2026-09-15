//! `LANGUAGE.md §7.0`: a reserved word is special at the start of a line, and a name
//! everywhere else.

use vela_span::FileId;

fn errors(source: &str) -> Vec<String> {
    vela_syntax::parse(FileId::from_raw(0), source)
        .diagnostics
        .iter()
        .map(|d| format!("{}: {}", d.code.as_str(), d.message))
        .collect()
}

/// A reserved word names things.
#[test]
fn reserved_words_can_name_declarations() {
    assert!(errors("struct image:\n    art: str\n").is_empty());
    assert!(errors("label pause:\n    return\n").is_empty());
    assert!(errors("enum menu:\n    a\n").is_empty());
    assert!(errors("fn guard(x: int) -> int:\n    return x\n").is_empty());
}

/// A reserved word is read like any other name.
#[test]
fn reserved_words_can_be_read() {
    assert!(errors("label a:\n    var x = scene\n    return\n").is_empty());
    assert!(errors("label a:\n    var x = menu\n    return\n").is_empty());
    // And as a member.
    assert!(errors("label a:\n    var x = theme.fg\n    return\n").is_empty());
}

/// A reserved word is written like any other name.
#[test]
fn reserved_words_can_be_assigned() {
    assert!(
        errors("label a:\n    var scene = 1\n    scene = 2\n    return\n").is_empty(),
        "{:?}",
        errors("label a:\n    var scene = 1\n    scene = 2\n    return\n")
    );
}

/// **The leading word still decides the line.** `scene room` is a scene statement even though
/// `scene` is also a fine name — and the reason it can be is that the rest of the line is not
/// an assignment.
#[test]
fn a_leading_reserved_word_still_opens_its_statement() {
    assert!(errors("label a:\n    scene room\n    return\n").is_empty());
    assert!(errors("label a:\n    pause 1.0\n    return\n").is_empty());
    assert!(errors("label a:\n    menu:\n        \"go\":\n            return\n").is_empty());
}

/// A call to a variable named after a keyword is a statement, and still parses as one.
#[test]
fn a_call_to_a_reserved_name_is_an_expression_statement() {
    assert!(
        errors("label a:\n    var scene = 1\n    scene()\n    return\n").is_empty(),
        "{:?}",
        errors("label a:\n    var scene = 1\n    scene()\n    return\n")
    );
}

/// The expression keywords are still themselves: `true` is a boolean, not a variable.
#[test]
fn true_and_none_are_still_literals() {
    let parsed = vela_syntax::parse(
        FileId::from_raw(0),
        "label a:\n    var x = true\n    return\n",
    );
    assert!(parsed.diagnostics.is_empty());
}
