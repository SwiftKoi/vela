//! The parsed file.

use crate::lex::Comment;
use crate::tree::Item;

/// A parsed file: its top-level items, in source order.
#[derive(Debug, Default)]
pub struct Program {
    /// The items.
    pub items: Vec<Item>,
    /// Every comment in the file, in source order.
    ///
    /// Not part of the tree — no rule of the language is about a comment — but carried *with* it,
    /// because the parser is the only thing that has seen the whole text and because every tool
    /// that rewrites a file needs them (`crate::Comment`).
    pub comments: Vec<Comment>,
}

impl Program {
    /// A program with the given items and no comments yet.
    ///
    /// The parser's own constructor: comments are the lexer's, and the driver attaches them
    /// afterwards, because no rule of the grammar is written in terms of one.
    #[must_use]
    pub fn with_items(items: Vec<Item>) -> Self {
        Self {
            items,
            comments: Vec::new(),
        }
    }

    /// The comment block written directly above `item`, if there is one.
    ///
    /// This *is* the language's documentation convention (`LANGUAGE.md §7.0`), and it is a rule
    /// rather than a marker: the lines are comments, the last of them is the line before the
    /// declaration, and nothing separates them. A blank line ends a block — a comment set apart
    /// from what follows it is about the file, not about that declaration.
    #[must_use]
    pub fn documentation<'a>(&'a self, src: &str, item: &Item) -> Vec<&'a Comment> {
        let mut block: Vec<&Comment> = Vec::new();
        let mut edge = item.span().start() as usize;

        // Backwards: each comment must be one line break above whatever follows it.
        for comment in self.comments.iter().rev() {
            let (start, end) = (comment.span.start() as usize, comment.span.end() as usize);
            if end >= edge {
                // Written after the declaration — a trailing comment, or a later item's. Not part
                // of the block, and not the end of it either.
                continue;
            }
            if !one_line_break(&src[end..edge]) {
                break;
            }
            block.push(comment);
            edge = start;
        }

        block.reverse();
        block
    }
}

/// Whether two offsets are separated by one line break and nothing but horizontal whitespace.
fn one_line_break(between: &str) -> bool {
    let mut breaks = 0;
    for character in between.chars() {
        match character {
            '\n' => breaks += 1,
            '\r' | ' ' | '\t' => {}
            _ => return false,
        }
    }
    breaks == 1
}
