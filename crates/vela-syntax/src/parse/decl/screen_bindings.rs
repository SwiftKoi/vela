//! Parsing `key` and `timer`, the two lines that answer input (`SCREENS.md §2.3`).
//!
//! Its own file rather than part of `screen.rs`, for the reason `screen_use.rs` has one: a screen body
//! is a widget tree, and these two lines are not widgets at all — they place nothing. They are the
//! screen's answers to input, and they share a word (`action`) that no widget prop does.

use crate::lex::token::TokenKind;
use crate::parse::parser::Parser;
use crate::tree::ScreenLine;

impl Parser<'_> {
    /// `key <action> action <call>`.
    ///
    /// The semantic action is a *name*, not Ren'Py's string (`key "game_menu"`) — the same trade
    /// `style_prefix` makes, and the migrator's job to drop the quotes. `action` is the one word this
    /// position reads, so a binding is legible as "this action means that call".
    pub(crate) fn parse_screen_key(&mut self, start: vela_span::Span) -> Option<ScreenLine> {
        self.bump();
        let name = self.expect_name("a semantic action").unwrap_or_default();
        self.eat_keyword_word("action");
        let action = self.parse_expr();
        self.end_statement();
        Some(ScreenLine::Key {
            span: start.to(self.prev_span()),
            name,
            action,
        })
    }

    /// `timer <seconds> action <call> [repeat]`.
    ///
    /// `repeat` comes before `action`, which is the order Ren'Py writes it and the order that reads
    /// as one sentence: how long, whether again, and what.
    pub(crate) fn parse_screen_timer(&mut self, start: vela_span::Span) -> Option<ScreenLine> {
        self.bump();
        let seconds = self.parse_expr();
        let repeat = self.eat_keyword_word("repeat");
        self.eat_keyword_word("action");
        let action = self.parse_expr();
        self.end_statement();
        Some(ScreenLine::Timer {
            span: start.to(self.prev_span()),
            seconds,
            action,
            repeat,
        })
    }

    /// Consumes the current token when it is the bare word `word`.
    ///
    /// Used where a word introduces a value rather than being one: `key cancel action quit()` reads
    /// `action` as the middle word, and a line that omitted it would silently take `quit()` as
    /// whatever came next.
    fn eat_keyword_word(&mut self, word: &str) -> bool {
        if matches!(self.peek(), TokenKind::Ident) && self.text(self.current()) == word {
            self.bump();
            return true;
        }
        false
    }
}
