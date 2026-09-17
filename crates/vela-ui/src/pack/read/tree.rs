//! Reading the screen tree back: declarations, widgets, and the palette.
//!
//! One function per declaration and per line kind, so a tag and its fields sit together. Every
//! length comes from [`Reader::count`], which refuses a count larger than the container before
//! anything is allocated.

use vela_syntax::{Param, ScreenArg, ScreenDecl, ScreenLine, ScreenNode, Setting, StyleDecl, Type};

use crate::theme::{Palette, Rgb};

use super::cursor::Reader;

impl Reader<'_> {
    pub(super) fn screens(&mut self) -> Vec<ScreenDecl> {
        let count = self.count();
        let mut screens = Vec::with_capacity(count.min(1024));
        for _ in 0..count {
            screens.push(self.screen());
        }
        screens
    }

    fn screen(&mut self) -> ScreenDecl {
        let span = self.span();
        let name = self.string();
        let params = self.params();
        let body = self.lines();
        ScreenDecl {
            span,
            name,
            params,
            body,
        }
    }

    /// Parameters, which a lambda also carries (`read/expr.rs`).
    pub(super) fn params(&mut self) -> Vec<Param> {
        let count = self.count();
        let mut params = Vec::with_capacity(count.min(1024));
        for _ in 0..count {
            let span = self.span();
            let name = self.string();
            // The flag the writer set: a type tag follows only when one was written.
            let ty = self.flag().then(|| self.ty());
            let default = self.flag().then(|| self.expr());
            params.push(Param {
                span,
                name,
                ty,
                default,
            });
        }
        params
    }

    fn ty(&mut self) -> Type {
        match self.u8() {
            0 => {
                let span = self.span();
                let count = self.count();
                let mut path = Vec::with_capacity(count.min(1024));
                for _ in 0..count {
                    path.push(self.string());
                }
                Type::Named { span, path }
            }
            1 => {
                let span = self.span();
                Type::Optional {
                    span,
                    inner: Box::new(self.ty()),
                }
            }
            2 => {
                let span = self.span();
                Type::List {
                    span,
                    element: Box::new(self.ty()),
                }
            }
            3 => {
                let span = self.span();
                let key = Box::new(self.ty());
                let value = Box::new(self.ty());
                Type::Map { span, key, value }
            }
            4 => {
                let span = self.span();
                let count = self.count();
                let mut elements = Vec::with_capacity(count.min(1024));
                for _ in 0..count {
                    elements.push(self.ty());
                }
                Type::Tuple { span, elements }
            }
            _ => Type::Error { span: self.span() },
        }
    }

    fn lines(&mut self) -> Vec<ScreenLine> {
        let count = self.count();
        let mut lines = Vec::with_capacity(count.min(1024));
        for _ in 0..count {
            lines.push(self.line());
        }
        lines
    }

    fn line(&mut self) -> ScreenLine {
        match self.u8() {
            0 => {
                let span = self.span();
                let name = self.string();
                ScreenLine::Layer { span, name }
            }
            1 => {
                let span = self.span();
                let condition = self.expr();
                let body = self.lines();
                ScreenLine::If {
                    span,
                    condition,
                    body,
                }
            }
            3 => {
                let span = self.span();
                let name = self.string();
                let count = self.count();
                let mut args = Vec::with_capacity(count.min(1024));
                for _ in 0..count {
                    args.push(self.arg());
                }
                let body = self.lines();
                ScreenLine::Use {
                    span,
                    name,
                    args,
                    body,
                }
            }
            4 => ScreenLine::Transclude { span: self.span() },
            5 => {
                let span = self.span();
                let name = self.string();
                ScreenLine::StylePrefix { span, name }
            }
            _ => ScreenLine::Node(self.node()),
        }
    }

    fn node(&mut self) -> ScreenNode {
        let span = self.span();
        let name = self.string();
        let count = self.count();
        let mut args = Vec::with_capacity(count.min(1024));
        for _ in 0..count {
            args.push(self.arg());
        }
        let children = self.lines();
        ScreenNode {
            span,
            name,
            args,
            children,
        }
    }

    fn arg(&mut self) -> ScreenArg {
        match self.u8() {
            0 => ScreenArg::Value(self.expr()),
            _ => {
                let span = self.span();
                let name = self.string();
                let value = self.flag().then(|| self.expr());
                ScreenArg::Named { span, name, value }
            }
        }
    }

    pub(super) fn styles(&mut self) -> Vec<StyleDecl> {
        let count = self.count();
        let mut styles = Vec::with_capacity(count.min(1024));
        for _ in 0..count {
            let span = self.span();
            let name = self.string();
            let from = self.optional_string();
            let settings = self.settings();
            styles.push(StyleDecl {
                span,
                name,
                from,
                settings,
            });
        }
        styles
    }

    fn settings(&mut self) -> Vec<Setting> {
        let count = self.count();
        let mut settings = Vec::with_capacity(count.min(1024));
        for _ in 0..count {
            let span = self.span();
            let ty = self.optional_string();
            let key = self.string();
            let value = self.expr();
            settings.push(Setting {
                span,
                ty,
                key,
                value,
            });
        }
        settings
    }

    pub(super) fn palette(&mut self) -> Palette {
        let count = self.count();
        let mut colors = Vec::with_capacity(count.min(1024));
        for _ in 0..count {
            let token = self.string();
            let r = self.u8();
            let g = self.u8();
            let b = self.u8();
            colors.push((token, Rgb { r, g, b }));
        }
        Palette { colors }
    }
}
