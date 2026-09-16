//! Scanning a single lexeme: identifiers, numbers, strings, paths, and operators.

use crate::error;
use crate::lex::lexer::Lexer;
use crate::lex::token::{Keyword, TokenKind};

impl Lexer<'_> {
    /// Lexes one token.
    ///
    /// Returns `None` only after reporting and skipping an unrecognised byte, in which
    /// case the cursor has still advanced — the driver relies on that to terminate.
    pub(crate) fn lex_token(&mut self) -> Option<TokenKind> {
        let start = self.cursor.pos();
        let byte = self.cursor.peek()?;

        if byte.is_ascii_alphabetic() || byte == b'_' {
            return Some(self.lex_ident(start));
        }
        if byte.is_ascii_digit() {
            return Some(self.lex_number(start));
        }
        if byte == b'"' {
            return Some(self.lex_string(start));
        }
        if byte == b'@' {
            return Some(self.lex_path(start));
        }
        self.lex_operator(start, byte)
    }

    /// Lexes an identifier, then reclassifies it if it is a reserved word.
    fn lex_ident(&mut self, start: usize) -> TokenKind {
        self.cursor
            .bump_while(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
        let text = self.cursor.slice(start, self.cursor.pos());
        let kind = match Keyword::lookup(text) {
            Some(keyword) => TokenKind::Keyword(keyword),
            None => TokenKind::Ident,
        };
        self.push(kind, start, self.cursor.pos());
        kind
    }

    /// Lexes an integer or float literal.
    ///
    /// Hexadecimal is the one form that is not a variant of decimal scanning, so it is
    /// handled first and returns early.
    fn lex_number(&mut self, start: usize) -> TokenKind {
        if self.at_hex_prefix() {
            return self.lex_hexadecimal(start);
        }

        self.cursor
            .bump_while(|byte| byte.is_ascii_digit() || byte == b'_');

        let (mut is_float, already_reported) = self.lex_fraction();
        if self.lex_exponent() {
            is_float = true;
        }

        let end = self.cursor.pos();
        if !already_reported && !self.literal_fits(start, end, is_float) {
            self.diagnostics
                .push(error::numeric_overflow(self.file, start as u32, end as u32));
        }

        let kind = if is_float {
            TokenKind::Float
        } else {
            TokenKind::Int
        };
        self.push(kind, start, end);
        kind
    }

    /// Whether the cursor is on a `0x` prefix, which commits the literal to hexadecimal.
    fn at_hex_prefix(&self) -> bool {
        self.cursor.peek() == Some(b'0') && matches!(self.cursor.peek_at(1), Some(b'x' | b'X'))
    }

    /// Lexes a hexadecimal integer: `0xff_00_00`.
    fn lex_hexadecimal(&mut self, start: usize) -> TokenKind {
        self.cursor.bump();
        self.cursor.bump();
        self.cursor
            .bump_while(|byte| byte.is_ascii_hexdigit() || byte == b'_');

        let end = self.cursor.pos();
        if !self.literal_fits(start, end, false) {
            self.diagnostics
                .push(error::numeric_overflow(self.file, start as u32, end as u32));
        }

        self.push(TokenKind::Int, start, end);
        TokenKind::Int
    }

    /// Consumes a fractional part if present.
    ///
    /// Returns whether the literal became a float, and whether a diagnostic was already
    /// emitted for it — a trailing `.` is reported here, and re-reporting it as an
    /// unparseable number would be two errors for one mistake.
    fn lex_fraction(&mut self) -> (bool, bool) {
        if self.cursor.peek() != Some(b'.') {
            return (false, false);
        }

        match self.cursor.peek_at(1) {
            Some(byte) if byte.is_ascii_digit() => {
                self.cursor.bump();
                self.cursor
                    .bump_while(|byte| byte.is_ascii_digit() || byte == b'_');
                (true, false)
            }
            // `1.foo` is a field access on an integer, not a malformed float, so the
            // dot is left for the operator scanner.
            Some(byte) if byte.is_ascii_alphabetic() || byte == b'_' => (false, false),
            _ => {
                let dot = self.cursor.pos() as u32;
                self.cursor.bump();
                self.diagnostics
                    .push(error::trailing_decimal_point(self.file, dot, dot + 1));
                (true, true)
            }
        }
    }

    /// Consumes an exponent if present, reporting whether the literal became a float.
    fn lex_exponent(&mut self) -> bool {
        if !matches!(self.cursor.peek(), Some(b'e' | b'E')) {
            return false;
        }

        let save = self.cursor.pos();
        self.cursor.bump();
        if matches!(self.cursor.peek(), Some(b'+' | b'-')) {
            self.cursor.bump();
        }

        if self.cursor.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.cursor.bump_while(|byte| byte.is_ascii_digit());
            return true;
        }

        // `1e` is `1` followed by the identifier `e`, not a broken exponent.
        self.cursor.rewind_to(save);
        false
    }

    /// Whether the literal text parses as the type it will be given.
    fn literal_fits(&self, start: usize, end: usize, is_float: bool) -> bool {
        let text = self.cursor.slice(start, end).replace('_', "");
        if is_float {
            return text.parse::<f64>().is_ok();
        }
        match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
            // `0x` with no digits is malformed, and `from_str_radix` accepts an empty
            // string, so the emptiness is checked here rather than there.
            Some(digits) => !digits.is_empty() && i64::from_str_radix(digits, 16).is_ok(),
            None => text.parse::<i64>().is_ok(),
        }
    }

    /// Lexes a string literal.
    fn lex_string(&mut self, start: usize) -> TokenKind {
        self.consume_string(start);
        self.push(TokenKind::Str, start, self.cursor.pos());
        TokenKind::Str
    }

    /// Consumes a string literal beginning at its opening quote.
    ///
    /// Does not push a token, because a path literal reuses this. An unescaped line
    /// break ends the string rather than swallowing the rest of the file, so one missing
    /// quote produces one diagnostic.
    fn consume_string(&mut self, start: usize) {
        self.cursor.bump();
        loop {
            match self.cursor.bump() {
                Some(b'\\') => {
                    self.cursor.bump();
                }
                Some(b'"') => return,
                Some(b'\n' | b'\r') => {
                    self.cursor.rewind_to(self.cursor.pos() - 1);
                    self.report_unterminated(start);
                    return;
                }
                Some(_) => {}
                None => {
                    self.report_unterminated(start);
                    return;
                }
            }
        }
    }

    /// `E0008` — reports a string that ran off the end of its line or the file.
    fn report_unterminated(&mut self, start: usize) {
        let end = self.cursor.pos() as u32;
        self.diagnostics
            .push(error::unterminated_string(self.file, start as u32, end));
    }

    /// Lexes a path literal: `@"assets/forest.png"`.
    fn lex_path(&mut self, start: usize) -> TokenKind {
        self.cursor.bump();
        if self.cursor.peek() == Some(b'"') {
            self.consume_string(self.cursor.pos());
        } else {
            let offset = self.cursor.pos() as u32;
            self.diagnostics
                .push(error::unexpected_character(self.file, offset));
            self.cursor.bump();
        }
        self.push(TokenKind::Path, start, self.cursor.pos());
        TokenKind::Path
    }

    /// Lexes an operator or punctuation mark, or reports and skips an unrecognised byte.
    fn lex_operator(&mut self, start: usize, byte: u8) -> Option<TokenKind> {
        if let Some(kind) = self.two_byte_operator(start) {
            self.cursor.bump();
            self.cursor.bump();
            self.push(kind, start, self.cursor.pos());
            return Some(kind);
        }

        let kind = match byte {
            b'=' => TokenKind::Eq,
            b'<' => TokenKind::Lt,
            b'>' => TokenKind::Gt,
            b'+' => TokenKind::Plus,
            b'-' => TokenKind::Minus,
            b'*' => TokenKind::Star,
            b'/' => TokenKind::Slash,
            b'%' => TokenKind::Percent,
            b'?' => TokenKind::Question,
            b'!' => TokenKind::Bang,
            b'(' => TokenKind::LParen,
            b')' => TokenKind::RParen,
            b'[' => TokenKind::LBracket,
            b']' => TokenKind::RBracket,
            b'{' => TokenKind::LBrace,
            b'}' => TokenKind::RBrace,
            b',' => TokenKind::Comma,
            b'.' => TokenKind::Dot,
            b':' => TokenKind::Colon,
            _ => {
                let offset = start as u32;
                self.diagnostics
                    .push(error::unexpected_character(self.file, offset));
                self.cursor.bump();
                return None;
            }
        };

        self.cursor.bump();
        self.push(kind, start, self.cursor.pos());
        Some(kind)
    }

    /// Recognises a two-byte operator at `start`, if one is there.
    fn two_byte_operator(&self, start: usize) -> Option<TokenKind> {
        match self.cursor.slice(start, start + 2) {
            "==" => Some(TokenKind::EqEq),
            "!=" => Some(TokenKind::BangEq),
            "<=" => Some(TokenKind::Le),
            ">=" => Some(TokenKind::Ge),
            "+=" => Some(TokenKind::PlusEq),
            "-=" => Some(TokenKind::MinusEq),
            "*=" => Some(TokenKind::StarEq),
            "/=" => Some(TokenKind::SlashEq),
            "->" => Some(TokenKind::Arrow),
            "=>" => Some(TokenKind::FatArrow),
            "??" => Some(TokenKind::QuestionQuestion),
            _ => None,
        }
    }
}

/// Whether a whole string is a name the language can carry: `LANGUAGE.md §1`.
///
/// The lexer's own rule, stated for a string that is already in hand — a rename typed into an editor, a
/// name generated by a tool — rather than as a scan from a cursor. One definition of what a name may
/// look like, in the module that owns it: a tool with its own idea of an identifier is a tool that can
/// write something the parser will not read back.
#[must_use]
pub fn is_name(text: &str) -> bool {
    let mut characters = text.chars();
    match characters.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => return false,
    }
    characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}
