//! The declarations themselves.

use crate::error;
use crate::lex::token::{Keyword, TokenKind};
use crate::parse::parser::Parser;
use crate::tree::{
    CharacterDecl, EffectDecl, EnumDecl, FnDecl, ImageDecl, Item, LabelDecl, Program, ScreenDecl,
    StructDecl, StyleDecl, TransformDecl, Variant,
};
impl Parser<'_> {
    /// Parses a whole file.
    pub(crate) fn parse_program(&mut self) -> Program {
        let mut items = Vec::new();

        while !self.at_eof() {
            let before = self.pos;
            items.push(self.parse_item());
            // Recovery might not have moved the cursor if the offending token was itself
            // a plausible item start; this guarantees the loop terminates regardless.
            if self.pos == before {
                self.bump();
            }
        }

        Program::with_items(items)
    }

    /// Parses one top-level item.
    pub(crate) fn parse_item(&mut self) -> Item {
        let start = self.span();
        match self.peek() {
            TokenKind::Keyword(Keyword::Use) => Item::Use(self.parse_use()),
            TokenKind::Keyword(Keyword::Const) => Item::Const(self.parse_constant()),
            TokenKind::Keyword(Keyword::Default) => Item::Default(self.parse_default()),
            TokenKind::Keyword(Keyword::Struct) => Item::Struct(self.parse_struct()),
            TokenKind::Keyword(Keyword::Enum) => Item::Enum(self.parse_enum()),
            TokenKind::Keyword(Keyword::Character) => Item::Character(self.parse_character()),
            TokenKind::Keyword(Keyword::Image) => Item::Image(self.parse_image()),
            TokenKind::Keyword(Keyword::Transform) => Item::Transform(self.parse_transform()),
            TokenKind::Keyword(Keyword::Screen) => Item::Screen(self.parse_screen()),
            TokenKind::Keyword(Keyword::Style) => Item::Style(self.parse_style()),
            TokenKind::Keyword(Keyword::Test) => Item::Test(self.parse_test()),
            TokenKind::Keyword(Keyword::Theme) => Item::Theme(self.parse_theme()),
            TokenKind::Keyword(Keyword::Fn) => Item::Function(self.parse_fn()),
            TokenKind::Keyword(Keyword::Effect) => Item::Effect(self.parse_effect()),
            TokenKind::Keyword(Keyword::Label) => Item::Label(self.parse_label()),
            _ => {
                let token = self.current();
                let found = self.describe(token);
                self.diagnostics.push(error::unexpected(
                    token.span,
                    &found,
                    "a top-level declaration",
                ));
                self.recover_item();
                Item::Error { span: start }
            }
        }
    }
    /// Parses `struct NAME:` and its indented field list.
    pub(crate) fn parse_struct(&mut self) -> StructDecl {
        let start = self.span();
        self.bump();
        let name = self.expect_name("`struct`").unwrap_or_default();
        let fields = self.parse_fields();
        StructDecl {
            span: start.to(self.prev_span()),
            name,
            fields,
        }
    }

    /// Parses `enum NAME:` and its indented variant list.
    pub(crate) fn parse_enum(&mut self) -> EnumDecl {
        let start = self.span();
        self.bump();
        let name = self.expect_name("`enum`").unwrap_or_default();
        let mut variants = Vec::new();

        if self.enter_optional_block() {
            while !self.at(TokenKind::Dedent) && !self.at_eof() {
                let before = self.pos;
                let variant_start = self.span();
                if let Some(variant_name) = self.expect_name("a variant") {
                    let mut fields = Vec::new();
                    if self.at(TokenKind::LParen) {
                        self.bump();
                        fields = self.parse_params();
                        self.expect(TokenKind::RParen, "`)`");
                    }
                    variants.push(Variant {
                        span: variant_start.to(self.prev_span()),
                        name: variant_name,
                        fields,
                    });
                }
                self.end_statement();
                if self.pos == before {
                    self.bump();
                }
            }
            self.eat(TokenKind::Dedent);
        }

        EnumDecl {
            span: start.to(self.prev_span()),
            name,
            variants,
        }
    }
    /// Parses `character NAME:` and its indented settings.
    pub(crate) fn parse_character(&mut self) -> CharacterDecl {
        let start = self.span();
        self.bump();
        let name = self.expect_name("`character`").unwrap_or_default();
        let settings = self.parse_settings("`character`");
        CharacterDecl {
            span: start.to(self.prev_span()),
            name,
            settings,
        }
    }

    /// Parses `image NAME = expr`.
    pub(crate) fn parse_image(&mut self) -> ImageDecl {
        let start = self.span();
        self.bump();
        let (name, _) = self
            .parse_path("an image name")
            .unwrap_or_else(|| (Vec::new(), start));
        self.expect(TokenKind::Eq, "`=`");
        let value = self.parse_expr();
        let span = start.to(value.span());
        self.end_statement();
        ImageDecl { span, name, value }
    }

    /// Parses `transform NAME:` and its body.
    ///
    /// The body is an animation, parsed at M13; consuming it here means a file that
    /// contains transforms still parses today.
    pub(crate) fn parse_transform(&mut self) -> TransformDecl {
        let start = self.span();
        self.bump();
        let name = self.expect_name("`transform`").unwrap_or_default();
        let body = self.skip_body("`transform`");
        TransformDecl {
            span: start.to(body),
            name,
            body,
        }
    }

    /// Parses `screen NAME(params):` and its body.
    ///
    /// The body is a widget tree (), parsed here and checked later.
    pub(crate) fn parse_screen(&mut self) -> ScreenDecl {
        let start = self.span();
        self.bump();
        // A screen's name may be a reserved word: `pause`, `menu`, and `return` are all
        // natural names for a screen and all statements elsewhere. After `screen` there is no
        // ambiguity to resolve — the same reasoning as `audio.play` and `image`.
        let name = self.screen_prop_name().unwrap_or_default();
        let mut params = Vec::new();
        if self.at(TokenKind::LParen) {
            self.bump();
            params = self.parse_params();
            self.expect(TokenKind::RParen, "`)`");
        }
        let body = self.parse_screen_body("`screen`");
        ScreenDecl {
            span: start.to(self.prev_span()),
            name,
            params,
            body,
        }
    }

    /// Parses `style NAME [from BASE]:` and its settings.
    pub(crate) fn parse_style(&mut self) -> StyleDecl {
        let start = self.span();
        self.bump();
        let name = self.expect_name("`style`").unwrap_or_default();
        let from = if self.eat_keyword(Keyword::From) {
            self.expect_name("`from`")
        } else {
            None
        };
        let settings = self.parse_settings("`style`");
        StyleDecl {
            span: start.to(self.prev_span()),
            name,
            from,
            settings,
        }
    }

    /// Parses `fn NAME(params) [-> T]:` and its body.
    pub(crate) fn parse_fn(&mut self) -> FnDecl {
        let start = self.span();
        self.bump();
        let name = self.expect_name("`fn`").unwrap_or_default();
        self.expect(TokenKind::LParen, "`(`");
        let params = self.parse_params();
        self.expect(TokenKind::RParen, "`)`");
        let ret = if self.at(TokenKind::Arrow) {
            self.bump();
            Some(self.parse_type())
        } else {
            None
        };
        let body = self.parse_block("`fn`");
        FnDecl {
            span: start.to(self.prev_span()),
            name,
            params,
            ret,
            body,
        }
    }

    /// Parses `effect NAME(params) [-> T]`.
    ///
    /// No body, so no block: an effect is declared for the type checker and the runtime, and
    /// a declaration ended by `:` would look like something the author was expected to fill
    /// in.
    pub(crate) fn parse_effect(&mut self) -> EffectDecl {
        let start = self.span();
        self.bump();

        let Some(first) = self.expect_name("`effect`") else {
            return EffectDecl {
                span: start,
                path: Vec::new(),
                params: Vec::new(),
                ret: None,
            };
        };
        let mut path = vec![first];
        while self.at(TokenKind::Dot) {
            self.bump();
            if let Some(segment) = self.effect_segment() {
                path.push(segment);
            }
        }

        self.expect(TokenKind::LParen, "`(`");
        let params = self.parse_params();
        self.expect(TokenKind::RParen, "`)`");
        let ret = if self.at(TokenKind::Arrow) {
            self.bump();
            Some(self.parse_type())
        } else {
            None
        };

        let span = start.to(self.prev_span());
        // A declaration with no body still ends its own line, and the item loop expects the
        // parser to have consumed it.
        self.end_statement();
        EffectDecl {
            span,
            path,
            params,
            ret,
        }
    }

    /// One segment of a dotted effect name.
    ///
    /// A capability's leaf is often a word the language reserves — `audio.play`,
    /// `audio.stop`, `input.wait` — and refusing those would make the naming scheme
    /// unusable for exactly the capabilities that need it most. After a `.` there is no
    /// ambiguity to resolve, so a keyword is accepted where an identifier is expected.
    fn effect_segment(&mut self) -> Option<String> {
        if !matches!(self.peek(), TokenKind::Ident) {
            let token = self.current();
            if matches!(token.kind, TokenKind::Keyword(_)) {
                let text = self.text(token).to_string();
                self.bump();
                return Some(text);
            }
        }
        self.expect_name("an effect name")
    }

    /// Parses `label NAME:` and its body.
    pub(crate) fn parse_label(&mut self) -> LabelDecl {
        let start = self.span();
        self.bump();
        let name = self.expect_name("`label`").unwrap_or_default();
        let body = self.parse_block("`label`");
        LabelDecl {
            span: start.to(self.prev_span()),
            name,
            body,
        }
    }
}
