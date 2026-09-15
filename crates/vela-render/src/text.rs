//! Placing shaped text into a draw list.
//!
//! One implementation, used by everything in this crate that draws a run of text — the
//! dialogue, the stage placeholders, the menu. The arithmetic is not obvious (a glyph is
//! positioned from its baseline and its atlas rectangle, and the line box is what bounds it),
//! so a second copy would be a second chance to get it subtly wrong.

use vela_text::TextEngine;

use crate::draw::{Color, DrawList, GlyphQuad};

/// Where a run of text goes and how wide it may be.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct Placement {
    /// Font size in pixels.
    pub size: f32,
    /// Left edge.
    pub left: f32,
    /// Top of the text box.
    pub top: f32,
    /// The width it may wrap to.
    pub max_width: f32,
}

/// Lays out and emits one run of text, returning the height it occupied.
///
/// Returns `0.0` for a font that is not registered: an engine that cannot find its face draws
/// nothing rather than failing, because the caller has no better answer either.
pub(crate) fn place(
    text: &mut TextEngine,
    font: &str,
    draw: &mut DrawList,
    body: &str,
    at: Placement,
    color: Color,
) -> f32 {
    let Placement {
        size,
        left,
        top,
        max_width,
    } = at;
    let Some(layout) = text.layout(font, size, body, Some(max_width)) else {
        return 0.0;
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
    layout.height
}
