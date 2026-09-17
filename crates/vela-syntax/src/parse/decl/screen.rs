//! Parsing a `screen` body into a widget tree.
//!
//! `SCREENS.md §2`. The body is a tree by indentation, like every other block in the language,
//! with one wrinkle: a line is a *name followed by words*, and whether it names a widget or a
//! prop is not a question the parser can answer. See `tree/screen.rs` for why that is fine.

use crate::lex::token::{Keyword, TokenKind};
use crate::parse::parser::Parser;
use crate::tree::{ScreenElif, ScreenLine, ScreenNode};

impl Parser<'_> {
    /// Parses the indented body of a `screen`.
    pub(crate) fn parse_screen_body(&mut self, construct: &str) -> Vec<ScreenLine> {
        let mut lines = Vec::new();
        if !self.enter_block(construct) {
            return lines;
        }

        while !self.at(TokenKind::Dedent) && !self.at_eof() {
            let before = self.pos;
            if let Some(line) = self.parse_screen_line() {
                lines.push(line);
            }
            // Same guarantee as every other block loop: a parser that cannot make progress
            // would spin here forever, so forward motion does not depend on its correctness.
            if self.pos == before {
                self.bump();
            }
        }
        self.eat(TokenKind::Dedent);
        lines
    }

    /// Parses one line of a screen body.
    fn parse_screen_line(&mut self) -> Option<ScreenLine> {
        let start = self.span();

        if self.at(TokenKind::Keyword(Keyword::If)) {
            return self.parse_screen_if(start);
        }
        // `for` is a reserved word, unlike `layer` and `key`: it introduces a block and binds a name,
        // and a widget called `for` would be a line the statement grammar also reads.
        if self.at(TokenKind::Keyword(Keyword::For)) {
            return self.parse_screen_for(start);
        }
        // `use` is a keyword, and the only position where it is not a module import is here.
        if self.at(TokenKind::Keyword(Keyword::Use)) {
            return self.parse_screen_use(start);
        }
        if self.at_keyword_word("layer") {
            return self.parse_screen_layer(start);
        }
        // Contextual, like `layer`: a word only special in this position.
        if self.at_keyword_word("style_prefix") {
            return self.parse_style_prefix(start);
        }
        // Composition's other half: where a caller's block lands. Contextual like `layer` — the
        // word is only special in this position, so a project keeps it as a name everywhere else.
        if self.at_keyword_word("transclude") {
            self.bump();
            self.end_statement();
            return Some(ScreenLine::Transclude { span: start });
        }
        // Contextual too, and for the same reason: `key` and `timer` are ordinary words a widget
        // could be called, and only a line that starts with one is an input binding.
        if self.at_keyword_word("key") {
            return self.parse_screen_key(start);
        }
        if self.at_keyword_word("timer") {
            return self.parse_screen_timer(start);
        }
        // `pass` is the language's own empty statement, and a screen needs *something* indented
        // to have a body. Read as a widget it would be a line named `pass` and reported as an
        // unknown widget — an error about a screen that is doing exactly the right thing.
        if self.at_keyword_word("pass") {
            self.bump();
            self.end_statement();
            return None;
        }
        self.parse_screen_node(start).map(ScreenLine::Node)
    }

    /// `if <condition>:` and its body, then any `elif` arms and an `else`.
    ///
    /// The chain is parsed into one line rather than into sibling lines, which is the shape the
    /// statement form already has (`LANGUAGE.md §3`). `elif` and `else` are reserved words, so this
    /// loop is the only place a screen body consumes them: written anywhere else they are a name the
    /// widget vocabulary does not know, which is the honest report for a conditional with no `if`.
    fn parse_screen_if(&mut self, start: vela_span::Span) -> Option<ScreenLine> {
        self.bump();
        let condition = self.parse_expr();
        let body = self.parse_screen_body("`if`");

        let mut elifs = Vec::new();
        while self.at(TokenKind::Keyword(Keyword::Elif)) {
            let elif_start = self.span();
            self.bump();
            let elif_condition = self.parse_expr();
            let elif_body = self.parse_screen_body("`elif`");
            elifs.push(ScreenElif {
                span: elif_start.to(self.prev_span()),
                condition: elif_condition,
                body: elif_body,
            });
        }

        let else_body = if self.at(TokenKind::Keyword(Keyword::Else)) {
            self.bump();
            Some(self.parse_screen_body("`else`"))
        } else {
            None
        };

        Some(ScreenLine::If {
            span: start.to(self.prev_span()),
            condition,
            body,
            elifs,
            else_body,
        })
    }

    /// `style_prefix <name>`.
    ///
    /// A *name*, not a string: Vela names a style the way it names anything else (`style = body`), and
    /// the migrator's job is to drop Ren'Py's quotes — the same trade every other line here makes.
    fn parse_style_prefix(&mut self, start: vela_span::Span) -> Option<ScreenLine> {
        self.bump();
        let name = self.expect_name("a style prefix").unwrap_or_default();
        self.end_statement();
        Some(ScreenLine::StylePrefix {
            span: start.to(self.prev_span()),
            name,
        })
    }

    /// `layer <name>`.
    ///
    /// `layer` is not a keyword — it is only special in this position, and reserving it would
    /// stop a project calling a variable `layer` for no gain.
    fn parse_screen_layer(&mut self, start: vela_span::Span) -> Option<ScreenLine> {
        self.bump();
        let name = self.expect_name("a layer name").unwrap_or_default();
        self.end_statement();
        Some(ScreenLine::Layer { span: start, name })
    }

    /// A widget or a bare prop: `name [props] [:]`.
    fn parse_screen_node(&mut self, start: vela_span::Span) -> Option<ScreenNode> {
        // A widget name may be a reserved word: `image`, `bar`, and `input` are all in the
        // registry and all keywords elsewhere. Refusing them would make the registry's own
        // default set unwritable — the same problem `audio.play` had as an effect name.
        let name = self.screen_prop_name()?;
        let args = self.parse_screen_args();

        let children = if self.at(TokenKind::Colon) {
            self.parse_screen_body(&format!("`{name}`"))
        } else {
            self.end_statement();
            Vec::new()
        };

        Some(ScreenNode {
            span: start.to(self.prev_span()),
            name,
            args,
            children,
        })
    }

    /// A prop's name, which may be a reserved word — `at`, `size`, and `min` are all props and
    /// several of them read as keywords in other positions.
    pub(crate) fn screen_prop_name(&mut self) -> Option<String> {
        if matches!(self.peek(), TokenKind::Ident) {
            return self.expect_name("a prop");
        }
        let token = self.current();
        if matches!(token.kind, TokenKind::Keyword(_)) {
            let text = self.text(token).to_string();
            self.bump();
            return Some(text);
        }
        self.expect_name("a prop")
    }

    /// Whether the current token is the bare word `word`.
    fn at_keyword_word(&self, word: &str) -> bool {
        matches!(self.peek(), TokenKind::Ident) && self.text(self.current()) == word
    }
}
