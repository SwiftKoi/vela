//! The indentation stack.
//!
//! Vela is indentation-significant, so the lexer has to turn leading whitespace into
//! explicit `Indent`/`Dedent` tokens. Doing that with a stack here — rather than inline
//! in the lexer — keeps the two errors this can produce (`E0004`, `E0005`) in one place.

/// What the lexer should emit for a line at a given indentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndentAction {
    /// The line continues the current block.
    Same,
    /// The line opens a deeper block: emit one `Indent`.
    Indent,
    /// The line closes this many blocks: emit that many `Dedent`s.
    Dedent(usize),
}

/// Why a line's indentation was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndentError {
    /// A block's children are not all indented by the same step (`E0004`).
    InconsistentStep {
        /// The step the block's first child established.
        expected: u32,
        /// The step this line used.
        found: u32,
    },
    /// A dedent landed between two open levels (`E0005`).
    MismatchedDedent {
        /// The column the offending line started at.
        found: u32,
    },
}

/// One open indentation level.
#[derive(Debug)]
struct Level {
    /// The column this level starts at.
    indent: u32,
    /// The step this level's first child established, once it has one.
    ///
    /// Remembering the step per block is what makes `E0004` precise: a block indented
    /// by four columns and then by six is reported at the second line, not fifty lines
    /// later when something finally fails to parse.
    child_step: Option<u32>,
}

/// Tracks the open indentation levels of a file.
#[derive(Debug)]
pub struct IndentStack {
    levels: Vec<Level>,
}

impl IndentStack {
    /// Creates a stack with only the root level open.
    #[must_use]
    pub fn new() -> Self {
        Self {
            levels: vec![Level {
                indent: 0,
                child_step: None,
            }],
        }
    }

    /// The column the current block starts at.
    #[must_use]
    pub fn current(&self) -> u32 {
        self.levels.last().map_or(0, |level| level.indent)
    }

    /// How many blocks are open above the root.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.levels.len() - 1
    }

    /// Accounts for a line starting at `indent` columns.
    ///
    /// On success the stack is already updated, so the caller only has to emit tokens.
    pub fn advance(&mut self, indent: u32) -> Result<IndentAction, IndentError> {
        let current = self.current();

        if indent > current {
            return self.open(indent, current);
        }
        if indent == current {
            return Ok(IndentAction::Same);
        }
        self.close(indent)
    }

    /// Opens a deeper block, checking the step against the block's established one.
    fn open(&mut self, indent: u32, current: u32) -> Result<IndentAction, IndentError> {
        let step = indent - current;
        let level = self
            .levels
            .last_mut()
            .expect("the root level is never popped");

        match level.child_step {
            Some(expected) if expected != step => {
                return Err(IndentError::InconsistentStep {
                    expected,
                    found: step,
                });
            }
            Some(_) => {}
            None => level.child_step = Some(step),
        }

        self.levels.push(Level {
            indent,
            child_step: None,
        });
        Ok(IndentAction::Indent)
    }

    /// Closes blocks until the stack matches `indent`, or reports a mismatch.
    fn close(&mut self, indent: u32) -> Result<IndentAction, IndentError> {
        let mut dedents = 0;
        while self.levels.len() > 1 && self.current() > indent {
            self.levels.pop();
            dedents += 1;
        }

        if self.current() != indent {
            // The line is shallower than where it was, but does not line up with any
            // enclosing block. Guessing a level here would silently mis-parse the rest
            // of the file, so this is reported and the stack is left as it is.
            return Err(IndentError::MismatchedDedent { found: indent });
        }
        Ok(IndentAction::Dedent(dedents))
    }
}

impl Default for IndentStack {
    fn default() -> Self {
        Self::new()
    }
}
