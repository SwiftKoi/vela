//! Comments: the part of a file the grammar does not see.
//!
//! A comment is not a token. It takes no part in the grammar, and every rule in the parser can be
//! written without knowing one was there — which is exactly why the lexer used to drop them. That
//! was wrong for two reasons that both live outside the parser: a formatter that cannot see a
//! comment deletes it, and a language server reads the comment block above a declaration as that
//! declaration's documentation (`LANGUAGE.md §7.0`). So they are *collected*, in source order,
//! beside the tokens rather than turned into tokens the parser must step over.

use vela_span::Span;

/// A formatting pragma: a comment that asks the formatter to keep its hands off a region
/// (`TOOLING.md §3`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pragma {
    /// `# fmt: off` — from here, the file is as the author left it.
    Off,
    /// `# fmt: on` — the region ends here.
    On,
}

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

    /// The formatting pragma this comment is, if it is one.
    ///
    /// The definition lives here, beside the comment it is about, because two readers need the same
    /// answer: the formatter, to decide which regions to leave alone, and the lint that reports a
    /// pragma so its use is visible in review. Two spellings of `"fmt: off"` is how a lint and a
    /// formatter come to disagree about whether a region was asked for.
    #[must_use]
    pub fn pragma(&self) -> Option<Pragma> {
        match self.text.trim() {
            "fmt: off" => Some(Pragma::Off),
            "fmt: on" => Some(Pragma::On),
            _ => None,
        }
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
