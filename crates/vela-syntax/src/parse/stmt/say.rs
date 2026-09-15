//! Dialogue: say statements and menus.

use vela_span::Span;

use crate::error;
use crate::lex::token::{Keyword, TokenKind};
use crate::parse::parser::Parser;
use crate::tree::{Expr, MenuChoice, MenuStmt, SayStmt, Stmt};
impl Parser<'_> {
    /// Parses a say statement, with the speaker already consumed.
    pub(crate) fn parse_say(&mut self, speaker: Option<String>, start: Span) -> Stmt {
        // Attributes sit between the speaker and the line, per the grammar's
        // `[ IDENT { IDENT } ] STRING` — so `eileen sad "Hi."` is a speaker, an image
        // attribute, then dialogue.
        let attributes = self.parse_attributes();

        let token = self.current();
        let line = if token.kind == TokenKind::Str {
            self.bump();
            self.parse_string(token)
        } else {
            let found = self.describe(token);
            self.diagnostics
                .push(error::expected_expression(token.span, &found));
            Expr::Error { span: token.span }
        };

        let options = self.parse_say_options();
        let transition = self.parse_with_clause();
        let span = start.to(self.prev_span());
        self.end_statement();

        Stmt::Say(SayStmt {
            span,
            speaker,
            attributes,
            line,
            options,
            transition,
        })
    }

    /// Parses the bare names that select an image variant, e.g. `sad`.
    pub(crate) fn parse_attributes(&mut self) -> Vec<String> {
        let mut attributes = Vec::new();
        while self.at(TokenKind::Ident) {
            attributes.push(self.expect_name("an attribute").unwrap_or_default());
        }
        attributes
    }

    /// Parses a say statement's `(key=value, ...)` options.
    pub(crate) fn parse_say_options(&mut self) -> Vec<(String, Expr)> {
        let mut options = Vec::new();
        if !self.at(TokenKind::LParen) {
            return options;
        }
        self.bump();

        while !self.at(TokenKind::RParen) && !self.at_eof() {
            let before = self.pos;
            if let Some(key) = self.expect_name("an option") {
                if self.eat(TokenKind::Eq) {
                    let value = self.parse_expr();
                    options.push((key, value));
                }
            }
            if self.pos == before {
                self.bump();
                break;
            }
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }

        self.expect(TokenKind::RParen, "`)`");
        options
    }

    /// Parses a trailing `with NAME`, if present.
    pub(crate) fn parse_with_clause(&mut self) -> Option<String> {
        if !self.eat_keyword(Keyword::With) {
            return None;
        }
        self.expect_name("a transition")
    }

    /// Parses `menu` and its choices.
    pub(crate) fn parse_menu(&mut self) -> Stmt {
        let start = self.span();
        self.bump();

        let prompt = if self.at(TokenKind::Str) {
            let token = self.bump();
            Some(self.parse_string(token))
        } else {
            None
        };

        let mut choices = Vec::new();
        if self.enter_block("`menu`") {
            while !self.at(TokenKind::Dedent) && !self.at_eof() {
                let before = self.pos;
                let choice_start = self.span();

                if self.at(TokenKind::Str) {
                    let token = self.bump();
                    let text = self.parse_string(token);
                    let condition = if self.eat_keyword(Keyword::If) {
                        Some(self.parse_expr())
                    } else {
                        None
                    };
                    let body = self.parse_block("a choice");
                    choices.push(MenuChoice {
                        span: choice_start.to(self.prev_span()),
                        text,
                        condition,
                        body,
                    });
                }

                if self.pos == before {
                    self.bump();
                }
            }
            self.eat(TokenKind::Dedent);
        }

        Stmt::Menu(MenuStmt {
            span: start.to(self.prev_span()),
            prompt,
            choices,
        })
    }
}
