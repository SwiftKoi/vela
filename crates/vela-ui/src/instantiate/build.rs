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

use vela_syntax::{ScreenArg, ScreenLine, ScreenNode};

use super::compose::Compose;
use super::props::{apply_args, apply_bare_prop, measure_text};
use crate::eval::{Args, Ctx, eval, number};
use crate::props::SizeSpec;
use crate::tree::{Kind, Node, Size};
use crate::widgets::{Category, Widget};

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
    let children = build_lines(body, ctx, args, Compose::default(), text, font, max_width);
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
    compose: Compose<'_>,
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
                    out.extend(build_lines(body, ctx, args, compose, text, font, max_width));
                }
            }
            // Composition's two halves. `use` expands a screen here, with its own parameters bound
            // from this call's arguments; `transclude` is where the block *this* body was handed is
            // placed. Which of the two a screen has is what makes it a caller or a wrapper.
            ScreenLine::Use {
                name,
                args: call,
                body,
                ..
            } => {
                if let Some((callee, bound, inner)) =
                    compose.expand(ctx.screens, name, call, args, body)
                {
                    out.extend(build_lines(
                        &callee.body,
                        ctx,
                        &bound,
                        inner,
                        text,
                        font,
                        max_width,
                    ));
                }
            }
            ScreenLine::Transclude { .. } => {
                // The block is built with the scope it was written in, and with no pane of its own:
                // it *is* the pane, so a `transclude` written inside a block places nothing. Nesting
                // one that far is a chain three `use`s deep, and nothing asks for it.
                if let Some((lines, pane_args)) = compose.block() {
                    out.extend(build_lines(
                        lines,
                        ctx,
                        pane_args,
                        Compose::default(),
                        text,
                        font,
                        max_width,
                    ));
                }
            }
            ScreenLine::Node(node) => {
                if ctx.registry.get(&node.name).is_some() {
                    out.push(build_node(node, ctx, args, compose, text, font, max_width));
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
    compose: Compose<'_>,
    text: &mut vela_text::TextEngine,
    font: &str,
    max_width: f32,
) -> Node {
    let widget = ctx.registry.get(&node.name);
    let leaf = widget.is_some_and(|widget| widget.category == Category::Leaf);
    let mut built = Node::new(kind_of(node), Vec::new());
    apply_args(&mut built, &node.args, ctx, args, leaf);

    for line in &node.children {
        apply_line(
            line, widget, &mut built, ctx, args, compose, text, font, max_width,
        );
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
    compose: Compose<'_>,
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
                        compose,
                        text,
                        font,
                        max_width,
                    );
                }
            }
        }
        // A widget's children may be a composition, which expands to however many nodes the used
        // screen draws — so it is built as a list and appended, not turned into one child.
        ScreenLine::Use {
            name,
            args: call,
            body,
            ..
        } => {
            if let Some((callee, bound, inner)) =
                compose.expand(ctx.screens, name, call, args, body)
            {
                parent.children.extend(build_lines(
                    &callee.body,
                    ctx,
                    &bound,
                    inner,
                    text,
                    font,
                    max_width,
                ));
            }
        }
        ScreenLine::Transclude { .. } => {
            if let Some((lines, pane_args)) = compose.block() {
                parent.children.extend(build_lines(
                    lines,
                    ctx,
                    pane_args,
                    Compose::default(),
                    text,
                    font,
                    max_width,
                ));
            }
        }
        ScreenLine::Node(child) => {
            if ctx.registry.get(&child.name).is_some() {
                parent
                    .children
                    .push(build_node(child, ctx, args, compose, text, font, max_width));
            } else {
                apply_bare_prop(parent, parent_widget, child, ctx, args);
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
