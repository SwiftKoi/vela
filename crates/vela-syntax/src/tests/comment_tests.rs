//! Comments: that they survive lexing, and what one above a declaration means.
//!
//! These pin the two properties a tool depends on. A formatter cannot see a comment the lexer
//! dropped, so the first is that they are all there; a language server needs a documentation
//! convention, so the second is the rule that decides which comment documents what.

use vela_span::FileId;

use crate::{Item, parse};

/// The comments of a file, rendered as written.
fn comments(src: &str) -> Vec<String> {
    parse(FileId::from_raw(0), src)
        .program
        .comments
        .iter()
        .map(crate::Comment::rendered)
        .collect()
}

/// The documentation attached to the first item of a file.
fn doc_of_first(src: &str) -> Vec<String> {
    let parsed = parse(FileId::from_raw(0), src);
    let Some(item) = parsed.program.items.first() else {
        panic!("the fixture has no items");
    };
    parsed
        .program
        .documentation(src, item)
        .into_iter()
        .map(crate::Comment::rendered)
        .collect()
}

#[test]
fn every_comment_is_kept_in_source_order() {
    let src = "# first\nlabel a:\n    \"x\"  # trailing\n    # inside\n    return\n";
    assert_eq!(comments(src), vec!["# first", "# trailing", "# inside"]);
}

/// A comment takes no part in the grammar, so it changes nothing about the tree.
#[test]
fn a_comment_does_not_change_what_the_fixture_parses_to() {
    let bare = parse(FileId::from_raw(0), "label a:\n    \"x\"\n    return\n");
    let commented = parse(
        FileId::from_raw(0),
        "# hi\nlabel a:\n    \"x\" # there\n    return\n",
    );

    assert!(commented.diagnostics.is_empty());
    assert_eq!(bare.program.items.len(), commented.program.items.len());
    assert!(matches!(commented.program.items[0], Item::Label(_)));
}

#[test]
fn the_block_above_a_declaration_is_its_documentation() {
    let src = "# What this does.\n# And why.\nlabel a:\n    return\n";
    assert_eq!(doc_of_first(src), vec!["# What this does.", "# And why."]);
}

/// A comment set apart from what follows is about the file, not about the declaration.
#[test]
fn a_blank_line_ends_a_documentation_block() {
    let src = "# A note about the file.\n\n# What this does.\nlabel a:\n    return\n";
    assert_eq!(doc_of_first(src), vec!["# What this does."]);
}

#[test]
fn a_declaration_with_nothing_above_it_has_no_documentation() {
    assert!(doc_of_first("label a:\n    return\n").is_empty());
    // Nor does one that follows code rather than a comment.
    let src = "label b:\n    return\n\nlabel a:\n    return\n";
    let parsed = parse(FileId::from_raw(0), src);
    let second = &parsed.program.items[1];
    assert!(parsed.program.documentation(src, second).is_empty());
}

/// A comment on the *same line* as the previous declaration documents nothing: it is about the
/// code it sits beside, and reading it as the next declaration's documentation would be wrong for
/// exactly that reason.
#[test]
fn a_trailing_comment_is_not_the_next_declarations_documentation() {
    let src = "label a:\n    return  # about this\n\nlabel b:\n    return\n";
    let parsed = parse(FileId::from_raw(0), src);
    let second = &parsed.program.items[1];

    assert!(parsed.program.documentation(src, second).is_empty());
    // The comment is still in the file, which is what stops a formatter deleting it.
    assert_eq!(parsed.program.comments.len(), 1);
}
