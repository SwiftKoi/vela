//! Parsing `default`, the line that gives a screen a variable (`SCREENS.md §2.5`).
//!
//! Its own file for the reason `screen_for.rs` has one: it is the second line in a body that *binds a
//! name*, and a binding is a scope question rather than a tree one. `default` is a reserved word —
//! unlike `layer`, `key`, and `timer` — because the module form already has it (`LANGUAGE.md §3`,
//! `default_decl`), so a screen's one is the same word doing the same thing closer to home.

use crate::lex::token::TokenKind;
use crate::parse::parser::Parser;
use crate::tree::ScreenLine;

impl Parser<'_> {
    /// `default <name> = <expr>`.
    ///
    /// The shape is the module form's, word for word (`LANGUAGE.md §3`), so an author who knows
    /// `default trust = 0` at the top of a file knows what one inside a screen is.
    pub(crate) fn parse_screen_default(&mut self, start: vela_span::Span) -> Option<ScreenLine> {
        self.bump();
        let name = self.expect_name("`default`").unwrap_or_default();
        self.expect(TokenKind::Eq, "`=`");
        let value = self.parse_expr();
        self.end_statement();
        Some(ScreenLine::Default {
            span: start.to(self.prev_span()),
            name,
            value,
        })
    }
}
