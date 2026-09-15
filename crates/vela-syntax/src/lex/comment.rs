//! Comments: the part of a file the grammar does not see.
//!
//! A comment is not a token. It takes no part in the grammar, and every rule in the parser can be
//! written without knowing one was there — which is exactly why the lexer used to drop them. That
//! was wrong for two reasons that both live outside the parser: a formatter that cannot see a
//! comment deletes it, and a language server reads the comment block above a declaration as that
//! declaration's documentation (`LANGUAGE.md §7.0`). So they are *collected*, in source order,
//! beside the tokens rather than turned into tokens the parser must step over.

use vela_span::Span;

/// One `#` comment, from the `#` to the end of its line.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Comment {
    /// The comment's span, including the `#` and excluding the line break.
    pub span: Span,
    /// The text after the `#`, with the line's trailing whitespace removed.
    ///
    /// Kept as written, including the space that usually follows the `#`: the only thing removed
    /// is trailing whitespace, which no reader wrote deliberately and a formatter must not emit.
    pub text: String,
}

impl Comment {
    /// The comment as it was written, for a reader.
    #[must_use]
    pub fn rendered(&self) -> String {
        format!("#{}", self.text)
    }

    /// Whether the comment sits on the same line as code that ends at `end`.
    ///
    /// A formatter needs the answer: a comment after a statement stays on that statement's line,
    /// and one on a line of its own keeps its own line. The answer is not in the comment, because
    /// "same line" is a question about what lies *between* two offsets — so it takes the source.
    #[must_use]
    pub fn is_trailing(&self, src: &str, end: usize) -> bool {
        if (self.span.start() as usize) < end {
            return false;
        }
        let between = src.get(end..self.span.start() as usize).unwrap_or("");
        !between.contains(['\n', '\r'])
    }
}
