//! The printer is faithful to the tree, including where two operators would otherwise be one.
//!
//! A formatter's whole promise is that its output means what its input meant. That is trivial when
//! every operator has one spelling and one precedence, and it is exactly where it gets interesting:
//! `not` and `!` are two operators, and for a while they arrived in the tree as one variant, so the
//! printer had no way to know which it was printing. `not a == b` and `!a == b` are different
//! programs — the first negates a comparison, the second compares a negation — and printing the
//! first as the second is a silent change of meaning.
//!
//! Written against `format`'s *output* rather than against the printer's internals, because the
//! output is the thing that has to re-parse to the same shape: whatever is printed, handed back to
//! the parser, must print identically.

use vela_span::FileId;

use crate::format;

/// The right-hand side of the one statement in a one-label file.
fn printed(source: &str) -> String {
    let source = format!("label a:\n    var v = {source}\n");
    let file = format(FileId::from_raw(0), &source).expect("the fixture parses");
    file.lines()
        .nth(1)
        .expect("a second line")
        .trim()
        .trim_start_matches("var v = ")
        .to_string()
}

/// Printing is idempotent, and the printed form is returned.
fn stable(source: &str) -> String {
    let once = printed(source);
    let twice = printed(&once);
    assert_eq!(
        once, twice,
        "`{source}` printed as `{once}`, which prints as `{twice}`"
    );
    once
}

#[test]
fn the_two_negations_are_the_operators_they_were_written_as() {
    assert_eq!(stable("not x"), "not x");
    assert_eq!(stable("!x"), "!x");
    assert_eq!(stable("not not x"), "not not x");
    assert_eq!(stable("-x"), "-x");
}

/// The case that made this a bug rather than a preference: the two spellings put the negation in
/// different places, so the same characters describe two different programs.
#[test]
fn a_negation_and_a_comparison_keep_their_own_grouping() {
    assert_eq!(stable("not a == b"), "not a == b");
    assert_eq!(stable("!a == b"), "!a == b");
}

/// A word operator needs the space that a symbol operator does not, and getting that wrong turns
/// `not x` into the identifier `notx`.
#[test]
fn a_word_operator_is_printed_with_its_space() {
    assert_eq!(printed("not x"), "not x");
    assert_eq!(printed("!x"), "!x");
}

/// Parentheses are part of the same promise: a tree that came from them prints them, and re-parsing
/// keeps the shape.
#[test]
fn parentheses_survive_the_round_trip() {
    assert_eq!(stable("(a == b)"), "(a == b)");
    assert_eq!(stable("!(a == b)"), "!(a == b)");
    assert_eq!(stable("not (a == b)"), "not (a == b)");
    assert_eq!(stable("(not a) == b"), "(not a) == b");
}

/// The three prefix operators are three printed forms; two that printed the same text would make one
/// of them unprintable, and the compiler cannot see that.
#[test]
fn the_prefix_operators_are_distinguishable() {
    let forms: std::collections::BTreeSet<String> =
        ["-x", "not x", "!x"].into_iter().map(printed).collect();

    assert_eq!(forms.len(), 3, "{forms:?}");
}
