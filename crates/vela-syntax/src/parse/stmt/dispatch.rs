//! Dispatching a statement on its leading token.

use crate::lex::token::{Keyword, TokenKind};
use crate::parse::parser::Parser;
use crate::tree::{AudioKind, StageKind, Stmt};
impl Parser<'_> {
    /// Parses one statement.
    pub(crate) fn parse_statement(&mut self) -> Stmt {
        // `LANGUAGE.md §7.0`: a reserved word is special at the start of a *line*, where it
        // opens a statement. The exception is a line that assigns to a variable which happens
        // to be named that word — `scene = 1` is an assignment, `scene room` is a scene.
        //
        // One lookahead tells them apart, the same way `at_named_say` tells narration from a
        // character speaking. The rule is *words are free in the middle of a line and
        // reserved at the front of one*, and this is the only place the front of a line is
        // ambiguous.
        if matches!(self.peek(), TokenKind::Keyword(_)) && self.at_keyword_expression() {
            return self.parse_expression_statement();
        }

        match self.peek() {
            TokenKind::Keyword(Keyword::Menu) => self.parse_menu(),
            TokenKind::Keyword(Keyword::Jump) => self.parse_jump(),
            TokenKind::Keyword(Keyword::Call) => self.parse_call(),
            TokenKind::Keyword(Keyword::Return) => self.parse_return(),
            TokenKind::Keyword(Keyword::Scene) => self.parse_stage(StageKind::Scene),
            TokenKind::Keyword(Keyword::Show) => self.parse_stage(StageKind::Show),
            TokenKind::Keyword(Keyword::Hide) => self.parse_stage(StageKind::Hide),
            TokenKind::Keyword(Keyword::With) => self.parse_with(),
            TokenKind::Keyword(Keyword::Play) => self.parse_audio(AudioKind::Play),
            TokenKind::Keyword(Keyword::Stop) => self.parse_audio(AudioKind::Stop),
            TokenKind::Keyword(Keyword::Queue) => self.parse_audio(AudioKind::Queue),
            TokenKind::Keyword(Keyword::Pause) => self.parse_pause(),
            TokenKind::Keyword(Keyword::Wait) => self.parse_wait(),
            TokenKind::Keyword(Keyword::If) => self.parse_if(),
            TokenKind::Keyword(Keyword::While) => self.parse_while(),
            TokenKind::Keyword(Keyword::For) => self.parse_for(),
            TokenKind::Keyword(Keyword::Match) => self.parse_match(),
            TokenKind::Keyword(Keyword::Var) => self.parse_var(),
            // A bare string is narration; a name followed by a string is that character
            // speaking. This is the only ambiguity in the statement grammar, and it
            // costs exactly one token of lookahead.
            TokenKind::Str => {
                let start = self.span();
                self.parse_say(None, start)
            }
            TokenKind::Ident if self.at_named_say() => {
                let start = self.span();
                let speaker = self.expect_name("a character");
                self.parse_say(speaker, start)
            }
            _ => self.parse_expression_statement(),
        }
    }

    /// Whether a line opening with a reserved word is really an expression.
    ///
    /// Walks past the name and anything that selects from it — `scene`, `scene.foo`,
    /// `scene[0]` — and asks whether an assignment or a call follows. Those are the only ways
    /// a line that *begins* with a reserved word can be about a variable rather than about
    /// the statement that word opens: `scene = 1` writes one, `scene()` calls one, and
    /// `scene room` is a scene.
    ///
    /// A bounded walk with no recursion, because the shape it recognises has none: a target is
    /// a name and a path.
    fn at_keyword_expression(&self) -> bool {
        let mut offset = 1;
        loop {
            match self.peek_at(offset) {
                TokenKind::Dot => offset += 2,
                TokenKind::LBracket => {
                    offset += 1;
                    while !matches!(self.peek_at(offset), TokenKind::RBracket | TokenKind::Eof) {
                        offset += 1;
                    }
                    offset += 1;
                }
                _ => break,
            }
        }
        matches!(
            self.peek_at(offset),
            TokenKind::Eq
                | TokenKind::PlusEq
                | TokenKind::MinusEq
                | TokenKind::StarEq
                | TokenKind::SlashEq
                // A call ends the name: `scene()` cannot be a scene statement, because a
                // scene statement's next word is an image and `(` is not one.
                | TokenKind::LParen
        )
    }

    /// Whether the cursor starts a named say statement.
    ///
    /// The shape is `IDENT { IDENT } STRING`: the first name is the speaker and the
    /// rest are image attributes. Looking only one token ahead would miss
    /// `eileen sad "Hi."` and parse it as an expression, so the scan runs past the
    /// attributes to find the string that makes this dialogue.
    pub(crate) fn at_named_say(&self) -> bool {
        let mut offset = 1;
        while self.peek_at(offset) == TokenKind::Ident {
            offset += 1;
        }
        self.peek_at(offset) == TokenKind::Str
    }

    /// Whether the current token is an identifier with exactly this spelling.
    ///
    /// Used for the words the grammar treats contextually rather than reserving, such
    /// as the `loop` and `fade` modifiers on `play`.
    pub(crate) fn at_word(&self, word: &str) -> bool {
        self.at(TokenKind::Ident) && self.text(self.current()) == word
    }

    /// Whether the cursor is at a token that ends the current statement.
    pub(crate) fn ends_statement(&self) -> bool {
        self.peek().ends_statement()
    }
}
