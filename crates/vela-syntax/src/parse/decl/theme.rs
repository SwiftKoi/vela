//! Parsing `theme` declarations.
//!
//! In its own file because the item parser's impl block has a line budget and this is a
//! self-contained construct: a name and a body of typed settings.

use crate::parse::parser::Parser;
use crate::tree::ThemeDecl;

impl Parser<'_> {
    /// Parses `theme NAME:` and its settings.
    pub(crate) fn parse_theme(&mut self) -> ThemeDecl {
        let start = self.span();
        self.bump();
        let name = self.screen_prop_name().unwrap_or_default();
        let settings = self.parse_typed_settings("`theme`", true);
        ThemeDecl {
            span: start.to(self.prev_span()),
            name,
            settings,
        }
    }
}
