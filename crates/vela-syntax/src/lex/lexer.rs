//! The lexer driver: layout, line structure, and token accumulation.
//!
//! Scanning an individual lexeme lives in `lexeme.rs`. The split is by responsibility:
//! this file knows about *lines and blocks*, that one knows about *characters*.

use vela_diag::Diagnostic;
use vela_span::{FileId, Span};

use crate::error;
use crate::lex::cursor::Cursor;
use crate::lex::indent::{IndentAction, IndentStack};
use crate::lex::token::{Token, TokenKind};

/// The byte order mark, which Vela rejects (`E0001`).
const BOM: &str = "\u{feff}";

/// Everything lexing produced.
#[derive(Debug)]
pub struct LexResult {
    /// The tokens, always ending with `Eof`.
    pub tokens: Vec<Token>,
    /// Lexical diagnostics, in source order.
    pub diagnostics: Vec<Diagnostic>,
}

/// Lexes a whole file.
#[must_use]
pub fn lex(file: FileId, src: &str) -> LexResult {
    Lexer::new(file, src).run()
}

/// The lexing state.
///
/// Shared by the two `impl` blocks in this module and in `lexeme.rs`, which is why the
/// fields are crate-visible rather than private.
pub(crate) struct Lexer<'a> {
    /// The file being lexed, stamped onto every span.
    pub(crate) file: FileId,
    /// The read head.
    pub(crate) cursor: Cursor<'a>,
    /// Accumulated tokens.
    pub(crate) tokens: Vec<Token>,
    /// Accumulated diagnostics.
    pub(crate) diagnostics: Vec<Diagnostic>,
    /// Open indentation levels.
    indent: IndentStack,
    /// Depth of open bracket groups. Newlines inside one are not statement breaks.
    brackets: i32,
    /// Whether the next token begins a logical line.
    at_line_start: bool,
    /// Whether the current line has produced a token, and so needs a `Newline`.
    line_has_token: bool,
}

impl<'a> Lexer<'a> {
    fn new(file: FileId, src: &'a str) -> Self {
        Self {
            file,
            cursor: Cursor::new(src),
            tokens: Vec::new(),
            diagnostics: Vec::new(),
            indent: IndentStack::new(),
            brackets: 0,
            at_line_start: true,
            line_has_token: false,
        }
    }

    fn run(mut self) -> LexResult {
        self.reject_bom();

        while !self.cursor.is_eof() {
            if self.brackets == 0 && self.at_line_start && self.begin_line() {
                continue;
            }
            if !self.lex_one() {
                break;
            }
        }

        self.finish();
        LexResult {
            tokens: self.tokens,
            diagnostics: self.diagnostics,
        }
    }

    /// `E0001` — a leading byte order mark is consumed and reported.
    fn reject_bom(&mut self) {
        if self.cursor.starts_with(BOM) {
            self.diagnostics.push(error::bom(self.file));
            for _ in 0..BOM.len() {
                self.cursor.bump();
            }
        }
    }

    /// Handles the start of a logical line: indentation, and skipping lines that carry
    /// no tokens.
    ///
    /// Returns `true` if the line was consumed without producing anything, which is the
    /// case for blank and comment-only lines. Those must not affect indentation, or a
    /// stray blank line inside a block would silently close it.
    fn begin_line(&mut self) -> bool {
        let start = self.cursor.pos();
        let indent = self.measure_indent();

        match self.cursor.peek() {
            None => true,
            Some(b'\n' | b'\r') => {
                self.cursor.bump_newline();
                true
            }
            Some(b'#') => {
                self.skip_to_eol();
                self.cursor.bump_newline();
                true
            }
            _ => {
                self.at_line_start = false;
                self.apply_indent(indent, start, self.cursor.pos());
                false
            }
        }
    }

    /// Consumes leading whitespace, reporting tabs, and returns the width in columns.
    ///
    /// A tab counts as four columns so recovery lands somewhere sensible instead of
    /// cascading one diagnostic per following line.
    fn measure_indent(&mut self) -> u32 {
        let mut indent = 0;
        loop {
            match self.cursor.peek() {
                Some(b' ') => {
                    self.cursor.bump();
                    indent += 1;
                }
                Some(b'\t') => {
                    let offset = self.cursor.pos() as u32;
                    self.cursor.bump();
                    self.diagnostics
                        .push(error::tab_indentation(self.file, offset));
                    indent += 4;
                }
                _ => return indent,
            }
        }
    }

    /// Emits the layout tokens a line's indentation implies.
    fn apply_indent(&mut self, indent: u32, start: usize, end: usize) {
        match self.indent.advance(indent) {
            Ok(IndentAction::Indent) => self.push(TokenKind::Indent, start, end),
            Ok(IndentAction::Same) => {}
            Ok(IndentAction::Dedent(count)) => {
                for _ in 0..count {
                    self.push(TokenKind::Dedent, start, end);
                }
            }
            Err(problem) => self.diagnostics.push(error::indentation(
                self.file,
                start as u32,
                end as u32,
                problem,
            )),
        }
    }

    /// Lexes one token. Returns `false` at end of input.
    fn lex_one(&mut self) -> bool {
        loop {
            self.skip_inline_trivia();

            let Some(byte) = self.cursor.peek() else {
                return false;
            };
            if byte == b'\n' || byte == b'\r' {
                self.end_line();
                return true;
            }

            if let Some(kind) = self.lex_token() {
                self.account_bracket(kind);
                self.line_has_token = true;
                return true;
            }
            // An unrecognised byte was reported and skipped; try the next one.
        }
    }

    /// Tracks bracket depth. A closer with no opener is clamped rather than
    /// underflowing: the parser reports the mismatch, and the lexer must not panic.
    fn account_bracket(&mut self, kind: TokenKind) {
        if kind.opens_bracket() {
            self.brackets += 1;
        } else if kind.closes_bracket() {
            self.brackets = (self.brackets - 1).max(0);
        }
    }

    /// Consumes a line break, emitting a `Newline` if the line had content.
    fn end_line(&mut self) {
        let start = self.cursor.pos();
        if self.cursor.peek() == Some(b'\r') && self.cursor.peek_at(1) != Some(b'\n') {
            self.diagnostics
                .push(error::lone_carriage_return(self.file, start as u32));
        }
        self.cursor.bump_newline();

        if self.brackets > 0 {
            return;
        }
        if self.line_has_token {
            self.push(TokenKind::Newline, start, self.cursor.pos());
        }
        self.at_line_start = true;
        self.line_has_token = false;
    }

    /// Consumes spaces, tabs, comments, and backslash continuations.
    fn skip_inline_trivia(&mut self) {
        loop {
            match self.cursor.peek() {
                Some(b' ' | b'\t') => {
                    self.cursor.bump();
                }
                Some(b'#') => self.skip_to_eol(),
                Some(b'\\') if matches!(self.cursor.peek_at(1), Some(b'\n' | b'\r')) => {
                    self.cursor.bump();
                    self.cursor.bump_newline();
                }
                _ => return,
            }
        }
    }

    /// Consumes up to, but not including, the next line break.
    fn skip_to_eol(&mut self) {
        self.cursor
            .bump_while(|byte| byte != b'\n' && byte != b'\r');
    }

    /// Emits the trailing newline, any open dedents, and `Eof`.
    fn finish(&mut self) {
        let eof = self.cursor.pos();
        if self.line_has_token {
            self.push(TokenKind::Newline, eof, eof);
            self.line_has_token = false;
        }
        for _ in 0..self.indent.depth() {
            self.push(TokenKind::Dedent, eof, eof);
        }
        self.push(TokenKind::Eof, eof, eof);
    }

    /// Appends a token spanning `start..end`.
    pub(crate) fn push(&mut self, kind: TokenKind, start: usize, end: usize) {
        let span = Span::new(self.file, start as u32, end as u32);
        self.tokens.push(Token::new(kind, span));
    }
}
