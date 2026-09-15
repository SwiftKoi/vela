//! Shaping: text in, positioned glyphs out.
//!
//! Shaping is the step that turns characters into the glyphs a font actually draws them
//! with, which is not a bijection: one character can be several glyphs, several can be one
//! (a ligature), and which of those happens depends on the surrounding text. That is why
//! this is a separate pass rather than a lookup table, and why line breaking measures
//! *shaped* runs rather than counting characters.

use swash::shape::ShapeContext;

use crate::font::Font;

/// One glyph, positioned relative to the pen.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ShapedGlyph {
    /// The glyph id in the font.
    pub id: u16,
    /// Horizontal offset from the pen before drawing.
    pub x: f32,
    /// How far the pen moves afterwards.
    pub advance: f32,
    /// The byte offset in the source this glyph belongs to.
    ///
    /// Kept because everything above shaping — line breaking, emphasis ranges, carets —
    /// needs to map a glyph back to the text the author wrote, and a ligature makes that
    /// mapping many-to-one.
    pub cluster: u32,
}

/// A shaped string: the glyphs, in order, and how far the pen travelled.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct ShapedRun {
    /// The glyphs, in visual order.
    pub glyphs: Vec<ShapedGlyph>,
    /// The total advance, which is the width of the text.
    pub advance: f32,
}

impl ShapedRun {
    /// The advance up to the glyph starting at or after `limit`.
    ///
    /// Used by line breaking to measure a candidate line without re-shaping it: the run is
    /// shaped once for the whole paragraph, and every candidate break is a prefix of it.
    #[must_use]
    pub fn advance_before(&self, limit: u32) -> f32 {
        let mut total = 0.0;
        for glyph in &self.glyphs {
            if glyph.cluster >= limit {
                break;
            }
            total += glyph.advance;
        }
        total
    }
}

/// Shapes `text` with `font` at `size`.
///
/// The context is passed in rather than created here: it owns the font cache, and a context
/// per string would re-parse the face every call.
#[must_use]
pub fn shape(font: &Font, size: f32, text: &str, context: &mut ShapeContext) -> ShapedRun {
    let mut shaper = Font::shaper(context, font, size);
    shaper.add_str(text);

    let mut run = ShapedRun::default();
    shaper.shape_with(|cluster| {
        for glyph in cluster.glyphs {
            run.glyphs.push(ShapedGlyph {
                id: glyph.id,
                x: glyph.x,
                advance: glyph.advance,
                cluster: cluster.source.start,
            });
            run.advance += glyph.advance;
        }
    });
    run
}
