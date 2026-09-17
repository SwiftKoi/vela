//! Parsing `for`, the line that draws children from data (`SCREENS.md §2.4`).
//!
//! Its own file for the reason `screen_use.rs` and `screen_bindings.rs` have one: a screen body is a
//! widget tree, and a `for` is the line that says how many times a piece of it is drawn. It is also
//! the one line in a body that *binds a name*, which is a scope question rather than a tree one.

use crate::lex::token::Keyword;
use crate::parse::parser::Parser;
use crate::tree::ScreenLine;

impl Parser<'_> {
    /// `for <name> in <expr>:` and its body.
    ///
    /// The shape is the statement form's, word for word (`LANGUAGE.md §3`) — one loop in the language
    /// rather than two, so an author who knows the story syntax knows this one.
    pub(crate) fn parse_screen_for(&mut self, start: vela_span::Span) -> Option<ScreenLine> {
        self.bump();
        let binding = self.expect_name("`for`").unwrap_or_default();
        self.expect_keyword(Keyword::In, "`in`");
        let iterable = self.parse_expr();
        let body = self.parse_screen_body("`for`");
        Some(ScreenLine::For {
            span: start.to(self.prev_span()),
            binding,
            iterable,
            body,
        })
    }
}
