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
use crate::tree::{Kind, Node};

/// The size text is drawn at when no `style` sets one.
const DEFAULT_SIZE: f32 = 24.0;

/// The colour of text with no `style`, matching the presenter's default body colour.
fn default_text() -> Color {
    Color::rgb(235, 235, 240)
}

/// Paints `root` and everything under it into `draw`.
///
/// `frame` must be the frame `layout` produced for `root`; the two walk together, one child
/// per child, and a mismatch is a bug in the caller rather than something to recover from.
pub fn paint(root: &Node, frame: &Frame, text: &mut TextEngine, font: &str, draw: &mut DrawList) {
    paint_node(root, frame, 0.0, 0.0, text, font, draw);
}

/// Paints one node, at the absolute origin its ancestors place it at.
fn paint_node(
    node: &Node,
    frame: &Frame,
    parent_x: f32,
    parent_y: f32,
    text: &mut TextEngine,
    font: &str,
    draw: &mut DrawList,
) {
    let left = parent_x + frame.rect.x;
    let top = parent_y + frame.rect.y;

    if let Some(background) = node.paint.background
        && frame.rect.width > 0.0
        && frame.rect.height > 0.0
    {
        draw.push_rect(RectQuad {
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
            node.paint.color.unwrap_or_else(default_text),
            node.paint.size.unwrap_or(DEFAULT_SIZE),
            left,
            top,
            frame.rect.width,
            text,
            font,
            draw,
        );
    }

    for (child, child_frame) in node.children.iter().zip(&frame.children) {
        paint_node(child, child_frame, left, top, text, font, draw);
    }
}

/// Shapes one run of text and emits its glyphs, wrapped to `max_width`.
#[allow(clippy::too_many_arguments)]
fn paint_text(
    content: &str,
    color: Color,
    size: f32,
    left: f32,
    top: f32,
    max_width: f32,
    text: &mut TextEngine,
    font: &str,
    draw: &mut DrawList,
) {
    let Some(layout) = text.layout(font, size, content, Some(max_width)) else {
        return;
    };
    let (atlas_width, atlas_height) = text.atlas().size();

    for line in &layout.lines {
        let baseline = top + line.baseline;
        for glyph in &line.glyphs {
            let Some(rect) = text.glyph(font, size, glyph.id) else {
                continue;
            };
            draw.push_glyph(GlyphQuad {
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
