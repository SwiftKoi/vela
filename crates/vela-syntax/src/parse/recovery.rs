//! Error recovery.
//!
//! Recovery is what turns a parser from a gate into a tool: after a mistake, the parser
//! resynchronises at a boundary it can trust and keeps going, so one typo produces one
//! diagnostic rather than a cascade.
//!
//! The boundaries are the ones the token stream already guarantees — a `Newline` ends a
//! statement, and a `Dedent` returns to the enclosing block.

use crate::lex::token::{Keyword, TokenKind};
use crate::parse::parser::Parser;

impl Parser<'_> {
    /// Skips to the end of the current statement, balancing brackets on the way.
    pub(crate) fn recover_statement(&mut self) {
        let mut brackets = 0i32;
        while !self.at_eof() {
            match self.peek() {
                TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => brackets += 1,
                TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => brackets -= 1,
                TokenKind::Newline if brackets <= 0 => {
                    self.bump();
                    return;
                }
                // Never step past the end of the enclosing block.
                TokenKind::Dedent if brackets <= 0 => return,
                _ => {}
            }
            self.bump();
        }
    }

    /// Skips to the next top-level item, leaving any open blocks behind.
    pub(crate) fn recover_item(&mut self) {
        let mut blocks = 0i32;
        let mut consumed = false;

        while !self.at_eof() {
            match self.peek() {
                TokenKind::Indent => {
                    blocks += 1;
                    self.bump();
                    consumed = true;
                }
                TokenKind::Dedent => {
                    self.bump();
                    blocks -= 1;
                    consumed = true;
                    if blocks <= 0 {
                        return;
                    }
                }
                // Only a keyword that begins an item, and only once something has been
                // skipped, so recovery cannot stop before it starts.
                _ if blocks == 0 && consumed && self.at_item_start() => return,
                _ => {
                    self.bump();
                    consumed = true;
                }
            }
        }
    }

    /// Whether the current token can begin a top-level item.
    pub(crate) fn at_item_start(&self) -> bool {
        matches!(
            self.peek(),
            TokenKind::Keyword(
                Keyword::Use
                    | Keyword::Const
                    | Keyword::Default
                    | Keyword::Struct
                    | Keyword::Enum
                    | Keyword::Character
                    | Keyword::Image
                    | Keyword::Transform
                    | Keyword::Screen
                    | Keyword::Style
                    | Keyword::Theme
                    | Keyword::Fn
                    | Keyword::Label
            )
        )
    }
}
