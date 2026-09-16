//! What a line's words *mean*: one prop, one value, written into the node.
//!
//! Split from `build.rs` because the two answer different questions — "which node does this line
//! build" and "what does this word do to the node it is on" — and because one file answering both
//! passed the size budget. This half is the prop vocabulary's side of the parser's deliberate
//! ambiguity: a line is a name followed by words, and only the widget's schema says whether the name
//! is a widget or a prop.
//!
//! It is also where a *style* becomes paint, and so where a style's per-state values arrive
//! (`SCREENS.md §5.1`): the node keeps them apart so a state can be chosen at paint time.

use vela_syntax::{Expr, ScreenArg, ScreenNode};

use crate::actions::Action;
use crate::eval::{
    Args, Ctx, Value, action_of, anchor_of, anchor_word, color_of, name_of, number, style_paint,
    text_of,
};
use crate::props::{Anchor, SizeSpec};
use crate::tree::{Kind, Node, Size, State};

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
pub(super) enum PropValue<'a> {
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

/// A prop written on its own line, if the widget above accepts it.
///
/// The same ambiguity `check.rs` resolves with the same `accepts` question: `pad` is not a widget, and
/// `box` takes it. Its value is the line's `args`, in whichever of the two shapes the parser chose.
pub(super) fn apply_bare_prop(
    parent: &mut Node,
    parent_widget: Option<&crate::widgets::Widget>,
    child: &ScreenNode,
    ctx: &Ctx,
    values: &Args,
) {
    if parent_widget.is_some_and(|widget| widget.accepts(&child.name)) {
        apply_prop(
            parent,
            &child.name,
            PropValue::bare(&child.args),
            ctx,
            values,
        );
    }
}

/// Applies every inline argument to a node.
///
/// `leaf` is what resolves the parser's other ambiguity: `text name` and `pad 24` arrive as the
/// same shape — a name followed by a word — and only the widget's category says whether the
/// first bare name is content or a prop that happens to take no value. This is the same
/// carve-out `check.rs` makes when it decides whether an argument is a prop at all.
pub(super) fn apply_args(
    node: &mut Node,
    args: &[ScreenArg],
    ctx: &Ctx,
    values: &Args,
    leaf: bool,
) {
    for (index, arg) in args.iter().enumerate() {
        match arg {
            ScreenArg::Value(value) => set_content(node, value, values),
            ScreenArg::Named {
                name, value: None, ..
            } if index == 0 && leaf => set_content_name(node, name, values),
            ScreenArg::Named { name, value, .. } => {
                apply_prop(node, name, PropValue::inline(value.as_ref()), ctx, values);
            }
        }
    }
}

/// Applies one prop to a node, in whichever shape its value arrived.
fn apply_prop(node: &mut Node, name: &str, value: PropValue<'_>, ctx: &Ctx, values: &Args) {
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
                // The style's idle values, and whatever it says each state overrides (`SCREENS.md §5`).
                // The node keeps the two apart: which one is drawn depends on the focus cursor, and that
                // moves without the layout changing.
                let paint = style_paint(style, ctx);
                node.paint.background = paint.background.or(node.paint.background);
                node.paint.color = paint.color.or(node.paint.color);
                node.paint.size = paint.size.or(node.paint.size);
                for state in [State::Hover, State::Selected, State::Insensitive] {
                    if let Some(values) = paint.over(state).cloned() {
                        *node.paint.state_mut(state) = values;
                    }
                }
            }
        }
        "action" => {
            if let Some(action) = action_value(value, values) {
                node.action = Some(action);
            }
        }
        _ => {}
    }
}

/// The action a prop's value denotes.
///
/// Two ways to write one, and both have to land here. A call is an action written where it is used
/// (`action quit()`), which is the whole vocabulary. A bare name is an action the screen was *given*
/// (`screen confirm(message, yes_action, no_action)` … `action yes_action`) — and dropping that case
/// is what made the sample's confirm screen silently unclickable, with no diagnostic to say so.
fn action_value(value: PropValue<'_>, values: &Args) -> Option<Action> {
    if let Some(action) = value.expr().and_then(action_of) {
        return Some(action);
    }
    match value.name().and_then(|name| values.get(name)) {
        Some(Value::Action(action)) => Some(action.clone()),
        _ => None,
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
pub(super) fn measure_text(
    node: &mut Node,
    text: &mut vela_text::TextEngine,
    font: &str,
    max_width: f32,
) {
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
