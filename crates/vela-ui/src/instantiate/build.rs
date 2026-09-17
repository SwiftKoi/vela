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
use super::props::{apply_args, apply_bare_prop, apply_style, measure_text};
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
    // The style prefix this block's widgets fall back to (`SCREENS.md §5.2`): its own if it declares
    // one, otherwise the one it inherited. A block is the scope, so a nested `style_prefix` wins
    // inside itself and nowhere else.
    let compose = Compose {
        prefix: declared_prefix(lines).or(compose.prefix),
        ..compose
    };

    let mut out = Vec::new();
    for line in lines {
        match line {
            // A `layer` names where the screen draws and a `style_prefix` how its widgets look;
            // neither draws anything itself, and the prefix is read above. A binding places nothing
            // either: `instantiate::bindings` is what collects those.
            ScreenLine::Layer { .. }
            | ScreenLine::StylePrefix { .. }
            | ScreenLine::Key { .. }
            | ScreenLine::Timer { .. } => {}
            ScreenLine::If { .. } => {
                if let Some(arm) = arm_of(line, args) {
                    out.extend(build_lines(arm, ctx, args, compose, text, font, max_width));
                }
            }
            ScreenLine::Use { .. } | ScreenLine::Transclude { .. } => {
                if let Some(nodes) = composed(line, compose, ctx, args, text, font, max_width) {
                    out.extend(nodes);
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

    // The style the screen's prefix gives this widget, when the project declares one (`SCREENS.md
    // §5.2`). Applied *before* the arguments, so a `style = …` written on the node is applied after
    // and wins — which is the order the rule is stated in: the prefix is what a widget falls back to.
    if let Some(style) = prefixed_style(ctx, compose.prefix, &node.name) {
        apply_style(&mut built, style, ctx);
    }
    apply_args(&mut built, &node.args, ctx, args, leaf);

    // A widget's children are a block of their own, so a `style_prefix` among them scopes to them —
    // the same rule as any other block, and the reason the prefix is resolved here rather than only at
    // the top of a screen.
    let compose = Compose {
        prefix: declared_prefix(&node.children).or(compose.prefix),
        ..compose
    };
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
        ScreenLine::Layer { .. }
        | ScreenLine::StylePrefix { .. }
        | ScreenLine::Key { .. }
        | ScreenLine::Timer { .. } => {}
        ScreenLine::If { .. } => {
            if let Some(arm) = arm_of(line, args) {
                // A branch is a block, so a prefix declared inside it scopes to it — `build_lines`
                // re-reads a block it is handed, and this is the path that does not go through it.
                let compose = Compose {
                    prefix: declared_prefix(arm).or(compose.prefix),
                    ..compose
                };
                for child in arm {
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
        ScreenLine::Use { .. } | ScreenLine::Transclude { .. } => {
            if let Some(nodes) = composed(line, compose, ctx, args, text, font, max_width) {
                parent.children.extend(nodes);
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

/// The nodes a composition line draws, if it is one.
///
/// Composition's two halves: `use` expands the screen it names, with its own parameters bound from
/// this call's arguments, and `transclude` places the block *this* body was handed. Which of the two a
/// screen has is what makes it a caller or a wrapper. A list rather than a node, because either draws
/// however many nodes there are — which is why the two callers append instead of pushing.
fn composed(
    line: &ScreenLine,
    compose: Compose<'_>,
    ctx: &Ctx,
    args: &Args,
    text: &mut vela_text::TextEngine,
    font: &str,
    max_width: f32,
) -> Option<Vec<Node>> {
    match line {
        ScreenLine::Use {
            name,
            args: call,
            body,
            ..
        } => {
            let (callee, bound, inner) = compose.expand(ctx.screens, name, call, args, body)?;
            Some(build_lines(
                &callee.body,
                ctx,
                &bound,
                inner,
                text,
                font,
                max_width,
            ))
        }
        ScreenLine::Transclude { .. } => {
            // The block is built with the scope it was written in, and with no pane of its own: it
            // *is* the pane, so a `transclude` written inside a block places nothing. Nesting one that
            // far is a chain three `use`s deep, and nothing asks for it.
            let (lines, pane_args) = compose.block()?;
            Some(build_lines(
                lines,
                ctx,
                pane_args,
                Compose::default(),
                text,
                font,
                max_width,
            ))
        }
        _ => None,
    }
}

/// The arm an `if` draws: the first condition that holds, or the `else` when none does.
///
/// `None` for a line that is not an `if`, and for an `if` whose every arm fails. One decision in one
/// place, because three paths need it — a top-level line, one nested in a widget, and the input
/// bindings — and each of them would otherwise have to get the order and the short circuit right.
pub(super) fn arm_of<'a>(line: &'a ScreenLine, args: &Args) -> Option<&'a [ScreenLine]> {
    let ScreenLine::If {
        condition,
        body,
        elifs,
        else_body,
        ..
    } = line
    else {
        return None;
    };
    if eval(condition, args) {
        return Some(body);
    }
    for clause in elifs {
        if eval(&clause.condition, args) {
            return Some(&clause.body);
        }
    }
    else_body.as_deref()
}

/// The style prefix a block declares, if it declares one (`SCREENS.md §5.2`).
///
/// The first line wins: a block that declares two prefixes is a mistake nothing reports — a
/// `style_prefix` is an ordinary line — and taking the first is the one a reader sees.
fn declared_prefix(lines: &[ScreenLine]) -> Option<&str> {
    lines.iter().find_map(|line| match line {
        ScreenLine::StylePrefix { name, .. } => Some(name.as_str()),
        _ => None,
    })
}

/// The style a prefix gives a widget, when the project declares one.
///
/// `say` and a `text` give `say_text`. A project that declares no such style falls back to the
/// widget's own defaults rather than being an error — which is what makes a prefix safe to write
/// before every style it names exists, and it is the rule Ren'Py has.
fn prefixed_style<'a>(ctx: &'a Ctx, prefix: Option<&str>, widget: &str) -> Option<&'a str> {
    let prefix = prefix?;
    let name = format!("{prefix}_{widget}");
    ctx.styles
        .iter()
        .find(|style| style.name == name)
        .map(|style| style.name.as_str())
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
