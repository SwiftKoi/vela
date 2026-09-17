//! Writing the screen tree: declarations, widgets, the palette, and the fonts.
//!
//! One function per declaration and per line kind, so a tag and the fields that follow it sit
//! together. The tags are the reader's, in `read/tree.rs`.

use vela_syntax::{Param, ScreenArg, ScreenDecl, ScreenLine, ScreenNode, Setting, StyleDecl, Type};

use crate::theme::{Fonts, Palette};

use super::codec::{Writer, count};

impl Writer {
    pub(super) fn screens(&mut self, screens: &[ScreenDecl]) {
        self.u32(count(screens.len()));
        for screen in screens {
            self.screen(screen);
        }
    }

    fn screen(&mut self, screen: &ScreenDecl) {
        self.span(screen.span);
        self.string(&screen.name);
        self.params(&screen.params);
        self.lines(&screen.body);
    }

    /// Parameters, which a lambda also carries (`write/expr.rs`).
    pub(super) fn params(&mut self, params: &[Param]) {
        self.u32(count(params.len()));
        for param in params {
            self.param(param);
        }
    }

    fn param(&mut self, param: &Param) {
        self.span(param.span);
        self.string(&param.name);
        // A presence flag, because a parameter may be written without a type (`LANGUAGE.md §3`):
        // the flag says whether the type tag follows.
        self.flag(param.ty.is_some());
        if let Some(ty) = &param.ty {
            self.ty(ty);
        }
        match &param.default {
            Some(value) => {
                self.flag(true);
                self.expr(value);
            }
            None => self.flag(false),
        }
    }

    fn ty(&mut self, ty: &Type) {
        match ty {
            Type::Named { span, path } => {
                self.u8(0);
                self.span(*span);
                self.u32(count(path.len()));
                for segment in path {
                    self.string(segment);
                }
            }
            Type::Optional { span, inner } => {
                self.u8(1);
                self.span(*span);
                self.ty(inner);
            }
            Type::List { span, element } => {
                self.u8(2);
                self.span(*span);
                self.ty(element);
            }
            Type::Map { span, key, value } => {
                self.u8(3);
                self.span(*span);
                self.ty(key);
                self.ty(value);
            }
            Type::Tuple { span, elements } => {
                self.u8(4);
                self.span(*span);
                self.u32(count(elements.len()));
                for element in elements {
                    self.ty(element);
                }
            }
            Type::Error { span } => {
                self.u8(5);
                self.span(*span);
            }
        }
    }

    fn lines(&mut self, lines: &[ScreenLine]) {
        self.u32(count(lines.len()));
        for line in lines {
            self.line(line);
        }
    }

    fn line(&mut self, line: &ScreenLine) {
        match line {
            ScreenLine::Layer { span, name } => {
                self.u8(0);
                self.span(*span);
                self.string(name);
            }
            ScreenLine::If {
                span,
                condition,
                body,
                elifs,
                else_body,
            } => {
                self.u8(1);
                self.span(*span);
                self.expr(condition);
                self.lines(body);
                self.u32(count(elifs.len()));
                for clause in elifs {
                    self.span(clause.span);
                    self.expr(&clause.condition);
                    self.lines(&clause.body);
                }
                match else_body {
                    Some(body) => {
                        self.flag(true);
                        self.lines(body);
                    }
                    None => self.flag(false),
                }
            }
            ScreenLine::Use {
                span,
                name,
                args,
                body,
            } => {
                self.u8(3);
                self.span(*span);
                self.string(name);
                self.u32(count(args.len()));
                for arg in args {
                    self.arg(arg);
                }
                self.lines(body);
            }
            ScreenLine::Transclude { span } => {
                self.u8(4);
                self.span(*span);
            }
            ScreenLine::StylePrefix { span, name } => {
                self.u8(5);
                self.span(*span);
                self.string(name);
            }
            ScreenLine::Key { span, name, action } => {
                self.u8(6);
                self.span(*span);
                self.string(name);
                self.expr(action);
            }
            ScreenLine::Timer {
                span,
                seconds,
                action,
                repeat,
            } => {
                self.u8(7);
                self.span(*span);
                self.expr(seconds);
                self.expr(action);
                self.flag(*repeat);
            }
            ScreenLine::Node(node) => {
                self.u8(2);
                self.node(node);
            }
        }
    }

    fn node(&mut self, node: &ScreenNode) {
        self.span(node.span);
        self.string(&node.name);
        self.u32(count(node.args.len()));
        for arg in &node.args {
            self.arg(arg);
        }
        self.lines(&node.children);
    }

    fn arg(&mut self, arg: &ScreenArg) {
        match arg {
            ScreenArg::Value(value) => {
                self.u8(0);
                self.expr(value);
            }
            ScreenArg::Named { span, name, value } => {
                self.u8(1);
                self.span(*span);
                self.string(name);
                match value {
                    Some(value) => {
                        self.flag(true);
                        self.expr(value);
                    }
                    None => self.flag(false),
                }
            }
        }
    }

    pub(super) fn styles(&mut self, styles: &[StyleDecl]) {
        self.u32(count(styles.len()));
        for style in styles {
            self.style(style);
        }
    }

    fn style(&mut self, style: &StyleDecl) {
        self.span(style.span);
        self.string(&style.name);
        self.optional_string(style.from.as_deref());
        self.settings(&style.settings);
    }

    fn settings(&mut self, settings: &[Setting]) {
        self.u32(count(settings.len()));
        for setting in settings {
            self.setting(setting);
        }
    }

    fn setting(&mut self, setting: &Setting) {
        self.span(setting.span);
        self.optional_string(setting.ty.as_deref());
        self.string(&setting.key);
        self.expr(&setting.value);
    }

    pub(super) fn palette(&mut self, palette: &Palette) {
        self.u32(count(palette.colors.len()));
        for (token, colour) in &palette.colors {
            self.string(token);
            self.u8(colour.r);
            self.u8(colour.g);
            self.u8(colour.b);
        }
    }

    pub(super) fn fonts(&mut self, fonts: &Fonts) {
        self.u32(count(fonts.tokens.len()));
        for (token, font) in &fonts.tokens {
            self.string(token);
            self.string(font);
        }
    }
}
