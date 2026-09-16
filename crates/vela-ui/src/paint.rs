//! The bridge from a laid-out widget tree to a draw list.
//!
//! `vela-render` owns the pixels and knows nothing about widgets; `vela-ui` owns the widgets
//! and, until now, knew nothing about pixels. This is the seam between them (`ARCHITECTURE.md
//! §5`, the rank-8 exception that lets `vela-ui` consume the renderer). It is deliberately
//! plain data in and plain data out: a `Node` plus its `Frame` becomes `RectQuad`s and
//! `GlyphQuad`s, so the mapping from a screen to what it draws is testable with no GPU and no
//! display — which a screenshot cannot be.
//!
//! Rectangles first, then text, because that is the order the draw list keeps them in and the
//! order a frame paints them.

use vela_render::{Color, DrawList, GlyphQuad, RectQuad};
use vela_text::TextEngine;

use crate::layout::Frame;
use crate::tree::{Kind, Node, State};

/// The size text is drawn at when no `style` sets one.
const DEFAULT_SIZE: f32 = 24.0;

/// The colour of text with no `style`, matching the presenter's default body colour.
fn default_text() -> Color {
    Color::rgb(235, 235, 240)
}

/// What a paint carries down the tree.
///
/// The text engine and the draw list travel together in every call, and so does the focus cursor —
/// which is why they are one value rather than three parameters threaded through every node.
struct Painter<'a> {
    text: &'a mut TextEngine,
    font: &'a str,
    draw: &'a mut DrawList,
    /// How many action-bearing nodes the walk has passed.
    seen: usize,
    /// Which of them the focus cursor is on, by the same numbering [`crate::focus::hotspots`] produces.
    focused: Option<usize>,
    /// Whether the walk is inside the focused control.
    ///
    /// A control's state belongs to everything it draws, not only to its own node: the words inside a
    /// focused button *are* that button, which is why Ren'Py's `hover_color` on a button colours its
    /// text. So the state is carried down the subtree rather than recomputed per node, and a text leaf
    /// inside the focused button draws `selected` although it has no action of its own.
    ///
    /// Only `selected` is reachable today: `hover` needs a pointer, which the host does not deliver
    /// (`SCREENS.md §11` — it resolves device events to semantic actions), and `insensitive` needs
    /// `enable_if` evaluated, which nothing does. Both are stored and resolvable; a state nothing can
    /// reach is a state nothing draws.
    selected: bool,
}

/// Paints `root` and everything under it into `draw`.
///
/// `frame` must be the frame `layout` produced for `root`; the two walk together, one child
/// per child, and a mismatch is a bug in the caller rather than something to recover from.
///
/// `focused` is the focused hotspot's index, counting action-bearing nodes in tree order — the same
/// numbering `focus::hotspots` produces, so a caller that has a focus cursor can pass it straight
/// through (`SCREENS.md §10`).
pub fn paint(
    root: &Node,
    frame: &Frame,
    text: &mut TextEngine,
    font: &str,
    draw: &mut DrawList,
    focused: Option<usize>,
) {
    let mut painter = Painter {
        text,
        font,
        draw,
        seen: 0,
        focused,
        selected: false,
    };
    paint_node(root, frame, 0.0, 0.0, &mut painter);
}

/// Paints one node, at the absolute origin its ancestors place it at.
///
/// The focus cursor is counted here rather than before the walk, so the numbering is the one
/// [`crate::focus::hotspots`] produces: action-bearing nodes, in tree order, parents before children.
/// A test asserts the two agree, because two walks that disagree would recolour the wrong button.
fn paint_node(node: &Node, frame: &Frame, parent_x: f32, parent_y: f32, painter: &mut Painter<'_>) {
    let left = parent_x + frame.rect.x;
    let top = parent_y + frame.rect.y;

    let enclosing = painter.selected;
    if node.action.is_some() {
        let index = painter.seen;
        painter.seen += 1;
        if painter.focused == Some(index) {
            painter.selected = true;
        }
    }
    let state = if painter.selected {
        State::Selected
    } else {
        State::Idle
    };
    let paint = node.paint.in_state(state);

    if let Some(background) = paint.background
        && frame.rect.width > 0.0
        && frame.rect.height > 0.0
    {
        painter.draw.push_rect(RectQuad {
            x: left,
            y: top,
            width: frame.rect.width,
            height: frame.rect.height,
            color: background,
        });
    }

    if let Kind::Text { text: content, .. } = &node.kind
        && !content.is_empty()
        && frame.rect.width > 0.0
    {
        paint_text(
            content,
            paint.color.unwrap_or_else(default_text),
            paint.size.unwrap_or(DEFAULT_SIZE),
            left,
            top,
            frame.rect.width,
            painter,
        );
    }

    for (child, child_frame) in node.children.iter().zip(&frame.children) {
        paint_node(child, child_frame, left, top, painter);
    }

    // The focused state is the *subtree's*, so a sibling of the focused control is not selected just
    // because the walk has been through it.
    painter.selected = enclosing;
}

/// Shapes one run of text and emits its glyphs, wrapped to `max_width`.
fn paint_text(
    content: &str,
    color: Color,
    size: f32,
    left: f32,
    top: f32,
    max_width: f32,
    painter: &mut Painter<'_>,
) {
    let Some(layout) = painter
        .text
        .layout(painter.font, size, content, Some(max_width))
    else {
        return;
    };
    let (atlas_width, atlas_height) = painter.text.atlas().size();

    for line in &layout.lines {
        let baseline = top + line.baseline;
        for glyph in &line.glyphs {
            let Some(rect) = painter.text.glyph(painter.font, size, glyph.id) else {
                continue;
            };
            painter.draw.push_glyph(GlyphQuad {
                x: left + glyph.x + rect.left as f32,
                y: baseline - rect.top as f32,
                width: rect.width as f32,
                height: rect.height as f32,
                uv: [
                    rect.x as f32 / atlas_width as f32,
                    rect.y as f32 / atlas_height as f32,
                    (rect.x + rect.width) as f32 / atlas_width as f32,
                    (rect.y + rect.height) as f32 / atlas_height as f32,
                ],
                color,
            });
        }
    }
}
