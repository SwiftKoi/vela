//! The parser's core: token access, expectations, and block structure.
//!
//! The parser never looks at whitespace. The lexer has already turned indentation into
//! `Indent`/`Dedent` tokens, so a block is just `Colon Newline Indent … Dedent` — which
//! is what lets an indentation-significant language use ordinary recursive descent.

use vela_diag::Diagnostic;
use vela_span::{FileId, Span};

use crate::error;
use crate::lex::token::{Keyword, Token, TokenKind};
use crate::tree::Stmt;

/// A recursive-descent parser over a token stream.
pub struct Parser<'a> {
    /// The file being parsed, stamped onto spans.
    pub(crate) file: FileId,
    /// The source text, for quoting tokens in diagnostics.
    pub(crate) src: &'a str,
    /// The token stream, which always ends with `Eof`.
    pub(crate) tokens: &'a [Token],
    /// How far into the stream we are.
    pub(crate) pos: usize,
    /// Diagnostics accumulated so far.
    pub(crate) diagnostics: Vec<Diagnostic>,
}

impl<'a> Parser<'a> {
    /// Creates a parser over an already-lexed token stream.
    pub(crate) fn new(file: FileId, src: &'a str, tokens: &'a [Token]) -> Self {
        Self {
            file,
            src,
            tokens,
            pos: 0,
            diagnostics: Vec::new(),
        }
    }

    /// Hands over the accumulated diagnostics.
    pub(crate) fn take_diagnostics(&mut self) -> Vec<Diagnostic> {
        std::mem::take(&mut self.diagnostics)
    }

    /// The kind of the current token.
    pub(crate) fn peek(&self) -> TokenKind {
        self.peek_at(0)
    }

    /// The kind of the token `offset` ahead.
    pub(crate) fn peek_at(&self, offset: usize) -> TokenKind {
        self.tokens
            .get(self.pos + offset)
            .map_or(TokenKind::Eof, |token| token.kind)
    }

    /// The current token.
    pub(crate) fn current(&self) -> Token {
        self.tokens.get(self.pos).copied().unwrap_or_else(|| {
            let end = self.src.len() as u32;
            Token::new(TokenKind::Eof, Span::new(self.file, end, end))
        })
    }

    /// The current token's span.
    pub(crate) fn span(&self) -> Span {
        self.current().span
    }

    /// The span of the most recently consumed token.
    pub(crate) fn prev_span(&self) -> Span {
        self.tokens
            .get(self.pos.saturating_sub(1))
            .map_or_else(|| self.span(), |token| token.span)
    }

    /// Consumes and returns the current token. Stops at `Eof` rather than running off
    /// the end, so a malformed file cannot panic the parser.
    pub(crate) fn bump(&mut self) -> Token {
        let token = self.current();
        if token.kind != TokenKind::Eof {
            self.pos += 1;
        }
        token
    }

    /// Whether the current token is `kind`.
    pub(crate) fn at(&self, kind: TokenKind) -> bool {
        self.peek() == kind
    }

    /// Whether the current token is a specific keyword.
    pub(crate) fn at_keyword(&self, keyword: Keyword) -> bool {
        self.peek() == TokenKind::Keyword(keyword)
    }

    /// Whether the cursor is at the end of input.
    pub(crate) fn at_eof(&self) -> bool {
        self.at(TokenKind::Eof)
    }

    /// Consumes the current token if it is `kind`.
    pub(crate) fn eat(&mut self, kind: TokenKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Consumes the current token if it is `keyword`.
    pub(crate) fn eat_keyword(&mut self, keyword: Keyword) -> bool {
        if self.at_keyword(keyword) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// The source text a token covers.
    pub(crate) fn text(&self, token: Token) -> &'a str {
        self.src
            .get(token.span.start() as usize..token.span.end() as usize)
            .unwrap_or("")
    }

    /// A short description of a token, for diagnostics.
    ///
    /// Words are quoted so the message points at the exact text; everything else uses
    /// the token kind's own description.
    pub(crate) fn describe(&self, token: Token) -> String {
        match token.kind {
            TokenKind::Ident | TokenKind::Keyword(_) | TokenKind::Int | TokenKind::Float => {
                format!("`{}`", self.text(token))
            }
            other => other.describe().to_string(),
        }
    }

    /// Consumes the current token if it is `kind`; otherwise reports and returns `None`.
    pub(crate) fn expect(&mut self, kind: TokenKind, expected: &str) -> Option<Token> {
        if self.at(kind) {
            return Some(self.bump());
        }
        let token = self.current();
        let found = self.describe(token);
        self.diagnostics
            .push(error::unexpected(token.span, &found, expected));
        None
    }

    /// Consumes the current token if it is `keyword`; otherwise reports.
    pub(crate) fn expect_keyword(&mut self, keyword: Keyword, expected: &str) -> bool {
        if self.eat_keyword(keyword) {
            return true;
        }
        let token = self.current();
        let found = self.describe(token);
        self.diagnostics
            .push(error::unexpected(token.span, &found, expected));
        false
    }

    /// Consumes the current token if it is a name.
    pub(crate) fn expect_name(&mut self, after: &str) -> Option<String> {
        if self.at(TokenKind::Ident) {
            let token = self.bump();
            return Some(self.text(token).to_string());
        }
        // A reserved word is a name here.
        //
        // Every caller of this function is in a position where a name is *expected* — after
        // `label`, after a `.`, as a field, as a parameter — and in those positions there is
        // no syntax left for a keyword to be. Refusing one is not caution, it is a hole:
        // `image`, `pause`, `menu`, `return`, and `theme` are all natural names for the things
        // they are natural names for, and each one was, at some point in this milestone,
        // unwritable. Four separate fixes went into four separate positions before it was
        // clear they were one bug.
        //
        // The guard against this being *too* permissive is that no caller relies on
        // `expect_name` to reject a keyword; the ones that branch on a token check `peek`
        // first, and the test suite is what says so.
        if matches!(self.peek(), TokenKind::Keyword(_)) {
            let token = self.bump();
            return Some(self.text(token).to_string());
        }
        let token = self.current();
        self.diagnostics
            .push(error::expected_name(token.span, after));
        None
    }

    /// Parses an indented block: a colon, a line break, and the statements inside.
    ///
    /// Returns an empty body on failure rather than propagating an error, so the
    /// construct that owns the block still appears in the tree and later phases can
    /// report on it.
    pub(crate) fn parse_block(&mut self, construct: &str) -> Vec<Stmt> {
        let mut body = Vec::new();
        if !self.enter_block(construct) {
            return body;
        }

        while !self.at(TokenKind::Dedent) && !self.at_eof() {
            let before = self.pos;
            body.push(self.parse_statement());
            // A statement parser that cannot make progress would spin here forever, so
            // the loop guarantees forward motion independently of its correctness.
            if self.pos == before {
                self.bump();
            }
        }
        self.eat(TokenKind::Dedent);
        body
    }

    /// Consumes `:` `Newline` `Indent`, reporting if there is no indented block.
    ///
    /// Returns whether a block is now open. The matching `Dedent` is left for the
    /// caller, which is the only party that knows where its own content ends.
    pub(crate) fn enter_block(&mut self, construct: &str) -> bool {
        if self.expect(TokenKind::Colon, "`:`").is_none() {
            return false;
        }
        self.eat(TokenKind::Newline);

        if self.eat(TokenKind::Indent) {
            return true;
        }
        let token = self.current();
        self.diagnostics
            .push(error::expected_block(token.span, construct));
        false
    }

    /// Consumes an indented block without parsing it, returning the span it covered.
    ///
    /// Used for constructs whose contents belong to a later milestone — a `screen` body
    /// is a widget tree (M7), a `transform` body is an animation (M13). Consuming the
    /// block here means a file containing them still parses today.
    pub(crate) fn skip_body(&mut self, construct: &str) -> Span {
        let start = self.span();
        if !self.enter_block(construct) {
            return start;
        }

        let mut depth = 1usize;
        let mut end = start;
        while depth > 0 && !self.at_eof() {
            match self.peek() {
                TokenKind::Indent => depth += 1,
                TokenKind::Dedent => depth -= 1,
                _ => {}
            }
            end = self.span();
            self.bump();
        }
        start.to(end)
    }

    /// Consumes a statement's terminating line break, if one is present.
    ///
    /// Constructs that end with a block have no trailing `Newline` — the block's
    /// `Dedent` ended them — so this is deliberately tolerant.
    pub(crate) fn end_statement(&mut self) {
        self.eat(TokenKind::Newline);
    }
}
