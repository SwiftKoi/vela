//! The whitespace the printer decides, and the whitespace it must not.
//!
//! The corpus round-trip proves the formatter is a fixed point; it cannot prove a fixture was not
//! *shaped* by a bug. One was: a blank line after a `transform` disappeared, and the one fixture that
//! had one had been written with the losing version, so the round trip was perfectly stable and
//! perfectly wrong. These tests state the rules in isolation, where a fixture cannot hide them.

use vela_span::FileId;

use crate::format;

/// The canonical form of a source that parses.
fn canonical(source: &str) -> String {
    format(FileId::from_raw(0), source).expect("the fixture parses")
}

#[test]
fn a_blank_line_between_items_is_kept() {
    let source = "const A: int = 1\n\nconst B: int = 2\n";
    assert_eq!(canonical(source), source);
}

/// The case that was broken: a `transform`'s body is consumed rather than parsed, so its span ends at
/// the *next* token — past the blank line — and measuring the gap from there made the blank line
/// invisible.
#[test]
fn a_blank_line_after_a_transform_is_kept() {
    let source = "transform slide_in:\n    x = 0.0\n\nscreen s:\n    text \"hi\"\n";
    assert_eq!(canonical(source), source);
}

#[test]
fn a_blank_line_inside_a_block_is_kept() {
    let source = "label a:\n    \"one\"\n\n    \"two\"\n";
    assert_eq!(canonical(source), source);
}

/// Blank lines are the author's, so none are invented: two items written adjacent stay adjacent.
#[test]
fn no_blank_line_is_invented() {
    let source = "const A: int = 1\nconst B: int = 2\n";
    assert_eq!(canonical(source), source);
}

/// And runs collapse to one, so a file cannot accumulate whitespace every time it is formatted.
#[test]
fn a_run_of_blank_lines_collapses_to_one() {
    let source = "const A: int = 1\n\n\n\nconst B: int = 2\n";
    assert_eq!(canonical(source), "const A: int = 1\n\nconst B: int = 2\n");
}

/// A file that ends with the canonical single newline, and no trailing blank lines.
#[test]
fn the_file_ends_with_one_newline() {
    assert_eq!(canonical("const A: int = 1\n\n\n"), "const A: int = 1\n");
}
