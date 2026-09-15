//! Turning a `screen` body into a widget tree.
//!
//! A `screen` declaration is a *function* (`SCREENS.md §2`): given its arguments and the state
//! they read, it produces a widget tree and mutates nothing. This module is that function. It
//! is the missing half of M7's "screen compile" — the parser produces the ambiguous shape and
//! the checker validates it, but until now nothing turned one into a [`Node`].
//!
//! The shape is what this file decides; *what an expression means* is [`crate::eval`]'s job.
//! Splitting them keeps each one readable: "which node does this line build" and "what is the
//! value of this argument" are different questions, and merging them made one file answer both.

use vela_syntax::{Expr, ScreenArg, ScreenLine, ScreenNode};

use crate::eval::{
    Args, Ctx, Value, action_of, anchor_of, anchor_word, color_of, eval, name_of, number,
    style_color, style_size, text_of,
};
use crate::props::{Anchor, SizeSpec};
use crate::tree::{Kind, Node, Size};
use crate::widgets::{Category, Widget};

/// The size text is drawn at when no `style` sets one.
const DEFAULT_SIZE: f32 = 24.0;

/// The value written after a prop name.
///
/// The parser produces two shapes for the same thing, and both have to land here. `box at
/// bottom` carries the value as an expression; a prop on its own line — `pad 24` — has its name
/// consumed as the *line's* name, so its value arrives either as a bare value (`24`) or as a
/// bare word the parser could not tell from a nested prop name (`bottom` in `align center`).
/// Normalizing here is what lets one `apply_prop` serve both.
#[derive(Clone, Copy)]
enum PropValue<'a> {
    /// A value written as an expression.
    Expr(&'a Expr),
    /// A bare word that is the value, not another prop.
    Word(&'a str),
    /// A flag with no value.
    None,
}

impl<'a> PropValue<'a> {
    /// A value written inline: `gap 8`, `style = body`.
    fn inline(value: Option<&'a Expr>) -> Self {
        value.map_or(Self::None, Self::Expr)
    }

    /// The value of a prop written on its own line.
    fn bare(args: &'a [ScreenArg]) -> Self {
        match args {
            [ScreenArg::Value(expr)] => Self::Expr(expr),
            [
                ScreenArg::Named {
                    name, value: None, ..
                },
            ] => Self::Word(name),
            [
                ScreenArg::Named {
                    value: Some(expr), ..
                },
            ] => Self::Expr(expr),
            _ => Self::None,
        }
    }

    /// A number, if the value is one.
    fn number(self) -> Option<f32> {
        match self {
            Self::Expr(expr) => number(expr),
            _ => None,
        }
    }

    /// An anchor, whether written as an expression or a bare word.
    fn anchor(self) -> Option<Anchor> {
        match self {
            Self::Expr(expr) => anchor_of(expr),
            Self::Word(word) => anchor_word(word),
            Self::None => None,
        }
    }

    /// A colour, resolved against the theme.
    fn color(self, ctx: &Ctx) -> Option<vela_render::Color> {
        match self {
            Self::Expr(expr) => color_of(expr, ctx),
            _ => None,
        }
    }

    /// A name, whether written as an expression or a bare word.
    fn name(self) -> Option<&'a str> {
        match self {
            Self::Expr(expr) => name_of(expr),
            Self::Word(word) => Some(word),
            Self::None => None,
        }
    }

    /// The value as an expression, if it is not a bare word.
    fn expr(self) -> Option<&'a Expr> {
        match self {
            Self::Expr(expr) => Some(expr),
            _ => None,
        }
    }
}

/// Evaluates a screen body to a widget tree.
///
/// The result is a full-window `stack` holding the body's top-level nodes: a screen is placed
/// in a layer that fills the frame, which is what lets a top-level `at bottom` mean the bottom
/// of the screen rather than the bottom of its own content.
#[must_use]
pub fn build(
    body: &[ScreenLine],
    ctx: &Ctx,
    args: &Args,
    text: &mut vela_text::TextEngine,
    font: &str,
    max_width: f32,
) -> Node {
    let children = build_lines(body, ctx, args, text, font, max_width);
    let mut root = Node::new(Kind::Stack, children);
    root.props.width = SizeSpec::Percent(1.0);
    root.props.height = SizeSpec::Percent(1.0);
    root
}

/// Builds the widget nodes of a block.
fn build_lines(
    lines: &[ScreenLine],
    ctx: &Ctx,
    args: &Args,
    text: &mut vela_text::TextEngine,
    font: &str,
    max_width: f32,
) -> Vec<Node> {
    let mut out = Vec::new();
    for line in lines {
        match line {
            ScreenLine::Layer { .. } => {}
            ScreenLine::If {
                condition, body, ..
            } => {
                if eval(condition, args) {
                    out.extend(build_lines(body, ctx, args, text, font, max_width));
                }
            }
            ScreenLine::Node(node) => {
                if ctx.registry.get(&node.name).is_some() {
                    out.push(build_node(node, ctx, args, text, font, max_width));
                }
            }
        }
    }
    out
}

/// Builds one widget, its props, and its children.
fn build_node(
    node: &ScreenNode,
    ctx: &Ctx,
    args: &Args,
    text: &mut vela_text::TextEngine,
    font: &str,
    max_width: f32,
) -> Node {
    let widget = ctx.registry.get(&node.name);
    let leaf = widget.is_some_and(|widget| widget.category == Category::Leaf);
    let mut built = Node::new(kind_of(node), Vec::new());
    apply_args(&mut built, &node.args, ctx, args, leaf);

    for line in &node.children {
        apply_line(line, widget, &mut built, ctx, args, text, font, max_width);
    }

    measure_text(&mut built, text, font, max_width);
    built
}

/// Applies one child line: a nested widget, a plausible bare prop, or nothing.
#[allow(clippy::too_many_arguments)]
fn apply_line(
    line: &ScreenLine,
    parent_widget: Option<&Widget>,
    parent: &mut Node,
    ctx: &Ctx,
    args: &Args,
    text: &mut vela_text::TextEngine,
    font: &str,
    max_width: f32,
) {
    match line {
        ScreenLine::Layer { .. } => {}
        ScreenLine::If {
            condition, body, ..
        } => {
            if eval(condition, args) {
                for child in body {
                    apply_line(
                        child,
                        parent_widget,
                        parent,
                        ctx,
                        args,
                        text,
                        font,
                        max_width,
                    );
                }
            }
        }
        ScreenLine::Node(child) => {
            if ctx.registry.get(&child.name).is_some() {
                parent
                    .children
                    .push(build_node(child, ctx, args, text, font, max_width));
            } else if parent_widget.is_some_and(|widget| widget.accepts(&child.name)) {
                // A prop written on its own line belongs to the widget above it — the
                // ambiguity `check.rs` resolves with the same `accepts` question. Its value
                // is the line's `args`, in whichever of the two shapes the parser chose.
                apply_prop(parent, &child.name, PropValue::bare(&child.args), ctx);
            }
        }
    }
}

/// The layout kind a widget name denotes.
fn kind_of(node: &ScreenNode) -> Kind {
    match node.name.as_str() {
        "box" | "absolute" | "button" => Kind::Box,
        "row" => Kind::Row,
        "column" => Kind::Column,
        "stack" => Kind::Stack,
        "flow" => Kind::Flow,
        "grid" => Kind::Grid {
            columns: columns_of(node),
        },
        "text" => Kind::Text {
            text: String::new(),
            size: Size::ZERO,
        },
        "spacer" => Kind::Spacer,
        // `image`, `bar`, `input`, and any plugin widget: a leaf whose size the paint layer
        // will refine. They lay out as a measured box because nothing yet measures them.
        _ => Kind::Measured { size: Size::ZERO },
    }
}

/// A grid's declared column count.
fn columns_of(node: &ScreenNode) -> usize {
    for arg in &node.args {
        let ScreenArg::Named {
            name,
            value: Some(value),
            ..
        } = arg
        else {
            continue;
        };
        if name == "columns" {
            if let Some(count) = number(value) {
                return count.max(1.0) as usize;
            }
        }
    }
    1
}

/// Applies every inline argument to a node.
///
/// `leaf` is what resolves the parser's other ambiguity: `text name` and `pad 24` arrive as the
/// same shape — a name followed by a word — and only the widget's category says whether the
/// first bare name is content or a prop that happens to take no value. This is the same
/// carve-out `check.rs` makes when it decides whether an argument is a prop at all.
fn apply_args(node: &mut Node, args: &[ScreenArg], ctx: &Ctx, values: &Args, leaf: bool) {
    for (index, arg) in args.iter().enumerate() {
        match arg {
            ScreenArg::Value(value) => set_content(node, value, values),
            ScreenArg::Named {
                name, value: None, ..
            } if index == 0 && leaf => set_content_name(node, name, values),
            ScreenArg::Named { name, value, .. } => {
                apply_prop(node, name, PropValue::inline(value.as_ref()), ctx);
            }
        }
    }
}

/// Applies one prop to a node, in whichever shape its value arrived.
fn apply_prop(node: &mut Node, name: &str, value: PropValue<'_>, ctx: &Ctx) {
    match name {
        "pad" => set_number(&mut node.props.pad, value),
        "gap" => set_number(&mut node.props.gap, value),
        "grow" => set_number(&mut node.props.grow, value),
        "align" => set_anchor(&mut node.props.align, value),
        "anchor" => set_anchor(&mut node.props.anchor, value),
        // `at bottom` reads as an anchor. `SCREENS.md §4.2` reserves `at` for a transform,
        // but the transform grammar is M13's and no screen can express one yet — so until it
        // can, honoring the anchor is the honest reading rather than silently dropping it.
        "at" => set_anchor(&mut node.props.anchor, value),
        "stretch_x" => node.props.width = SizeSpec::Percent(1.0),
        "stretch_y" => node.props.height = SizeSpec::Percent(1.0),
        "size" => {
            if let Some(size) = value.number() {
                node.props.width = SizeSpec::Fixed(size);
                node.props.height = SizeSpec::Fixed(size);
            }
        }
        "background" => {
            if let Some(color) = value.color(ctx) {
                node.paint.background = Some(color);
            }
        }
        "style" => {
            if let Some(style) = value.name() {
                node.paint.color = style_color(style, ctx).or(node.paint.color);
                node.paint.size = style_size(style, ctx.styles).or(node.paint.size);
            }
        }
        "action" => {
            if let Some(action) = value.expr().and_then(action_of) {
                node.action = Some(action);
            }
        }
        _ => {}
    }
}

/// Sets a leaf's text content from a value expression, e.g. `text "hi"`.
fn set_content(node: &mut Node, expr: &Expr, values: &Args) {
    set_text(node, text_of(expr, values));
}

/// Sets a leaf's text content from a bare parameter name, e.g. `text line`.
fn set_content_name(node: &mut Node, name: &str, values: &Args) {
    let text = values.get(name).map(Value::as_text).unwrap_or_default();
    set_text(node, text);
}

/// Writes resolved text into a text leaf, if it has none yet.
fn set_text(node: &mut Node, content: String) {
    if let Kind::Text { text, .. } = &mut node.kind {
        if text.is_empty() {
            *text = content;
        }
    }
}

/// Measures a text leaf once its style (and so its size) is known.
fn measure_text(node: &mut Node, text: &mut vela_text::TextEngine, font: &str, max_width: f32) {
    let Kind::Text {
        text: content,
        size,
    } = &mut node.kind
    else {
        return;
    };
    if content.is_empty() {
        return;
    }
    let size_px = node.paint.size.unwrap_or(DEFAULT_SIZE);
    if let Some(layout) = text.layout(font, size_px, content, Some(max_width)) {
        *size = Size::new(layout.width, layout.height);
    }
}

/// Sets an `f32` from a value, leaving it alone when there is none.
fn set_number(field: &mut f32, value: PropValue<'_>) {
    if let Some(number) = value.number() {
        *field = number;
    }
}

/// Sets an anchor from a value, leaving it alone when there is none.
fn set_anchor(field: &mut Anchor, value: PropValue<'_>) {
    if let Some(anchor) = value.anchor() {
        *field = anchor;
    }
}
