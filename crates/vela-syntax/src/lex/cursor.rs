//! A byte cursor over source text.
//!
//! Everything the lexer does is a sequence of peeks, bumps, and slices from here, which
//! keeps offset bookkeeping in one place instead of spread across the lexer.

/// A forward-only reader over `&str`.
///
/// The cursor works in bytes. Vela's identifiers, operators, and layout are all ASCII,
/// so byte-wise scanning is correct; only string *contents* can contain multi-byte
/// characters, and those are copied verbatim.
#[derive(Debug)]
pub struct Cursor<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    /// Creates a cursor at the start of `src`.
    #[must_use]
    pub fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
        }
    }

    /// The current byte offset.
    #[must_use]
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Whether the cursor has consumed everything.
    #[must_use]
    pub fn is_eof(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    /// The byte at the cursor without consuming it.
    #[must_use]
    pub fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    /// The byte `offset` bytes ahead without consuming anything.
    #[must_use]
    pub fn peek_at(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.pos + offset).copied()
    }

    /// Consumes and returns the byte at the cursor.
    pub fn bump(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.pos += 1;
        Some(byte)
    }

    /// Consumes `byte` if it is next, reporting whether it was.
    pub fn eat(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// Whether the remaining input starts with `needle`.
    #[must_use]
    pub fn starts_with(&self, needle: &str) -> bool {
        self.remaining().starts_with(needle)
    }

    /// The unconsumed input.
    #[must_use]
    pub fn remaining(&self) -> &'a str {
        self.src.get(self.pos..).unwrap_or("")
    }

    /// The bytes between two offsets.
    ///
    /// Returns an empty string if the range is not on a character boundary, so a
    /// malformed range degrades rather than panicking inside a lexer.
    #[must_use]
    pub fn slice(&self, start: usize, end: usize) -> &'a str {
        self.src.get(start..end).unwrap_or("")
    }

    /// Consumes bytes while `predicate` holds, returning how many were consumed.
    pub fn bump_while(&mut self, mut predicate: impl FnMut(u8) -> bool) -> usize {
        let start = self.pos;
        while let Some(byte) = self.peek() {
            if !predicate(byte) {
                break;
            }
            self.pos += 1;
        }
        self.pos - start
    }

    /// Consumes a line feed, reporting how many were consumed.
    ///
    /// A lone carriage return is not a line break: `\r\n` is normalised, but a bare
    /// `\r` is reported by the lexer rather than silently treated as a newline.
    pub fn bump_newline(&mut self) -> usize {
        let mut consumed = 0;
        if self.eat(b'\r') {
            consumed += 1;
        }
        if self.eat(b'\n') {
            consumed += 1;
        }
        consumed
    }

    /// Pushes `offset` back, so the most recent bumps can be re-scanned.
    pub fn rewind_to(&mut self, offset: usize) {
        self.pos = offset.min(self.bytes.len());
    }
}
