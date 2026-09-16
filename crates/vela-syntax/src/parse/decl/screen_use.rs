//! Parsing `use`, the line that includes another screen (`SCREENS.md §2.1`).
//!
//! Its own file rather than part of `screen.rs`, because a screen body is a widget tree and this is
//! the one line in it that is not a widget at all — it is a call to another declaration, and it has
//! its own argument grammar to prove it.

use crate::lex::token::TokenKind;
use crate::parse::parser::Parser;
use crate::tree::{ScreenArg, ScreenLine};

impl Parser<'_> {
    /// `use <screen> [( args )] [ : block ]`.
    ///
    /// The name is read as a prop name so that a screen called `pause` or `image` can be used here —
    /// a screen's name is a name in this position, exactly as it is after the `screen` keyword.
    pub(crate) fn parse_screen_use(&mut self, start: vela_span::Span) -> Option<ScreenLine> {
        self.bump();
        let name = self.screen_prop_name()?;

        let args = if self.at(TokenKind::LParen) {
            self.bump();
            let args = self.parse_use_args();
            self.expect(TokenKind::RParen, "`)`");
            args
        } else {
            Vec::new()
        };

        let body = if self.at(TokenKind::Colon) {
            self.parse_screen_body(&format!("`{name}`"))
        } else {
            self.end_statement();
            Vec::new()
        };

        Some(ScreenLine::Use {
            span: start.to(self.prev_span()),
            name,
            args,
            body,
        })
    }

    /// A `use` argument: an expression, or `name = expression`.
    ///
    /// Unlike a widget's arguments there is no ambiguity to preserve here. A screen call has named
    /// parameters and positional values and no notion of a prop, so a bare name is a *value*
    /// (`use file_slots(page)`) and only a name followed by `=` is a parameter. That is the same
    /// one-token lookahead that tells `text line style = body` apart, pointing at `=` rather than
    /// away from it — which is what keeps `use game_menu(title)` from reading as a parameter called
    /// `title`.
    fn parse_use_args(&mut self) -> Vec<ScreenArg> {
        let mut args = Vec::new();
        while !self.at(TokenKind::RParen) && !self.at_eof() {
            let before = self.pos;
            let start = self.span();

            if matches!(self.peek(), TokenKind::Ident) && self.next_is_another_name() {
                let name = self.expect_name("an argument name").unwrap_or_default();
                self.expect(TokenKind::Eq, "`=`");
                let value = self.parse_expr();
                let span = start.to(value.span());
                args.push(ScreenArg::Named {
                    span,
                    name,
                    value: Some(value),
                });
            } else {
                args.push(ScreenArg::Value(self.parse_expr()));
            }

            self.eat(TokenKind::Comma);
            if self.pos == before {
                self.bump();
                break;
            }
        }
        args
    }
}
