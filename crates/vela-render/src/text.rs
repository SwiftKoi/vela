//! Placing shaped text into a draw list.
//!
//! One implementation, used by everything in this crate that draws a run of text — the dialogue,
//! the stage placeholders, the menu. The arithmetic is not obvious (a glyph is positioned from its
//! baseline and its atlas rectangle, and the line box is what bounds it), so a second copy would be
//! a second chance to get it subtly wrong.
//!
//! # Text tags
//!
//! `BYTECODE.md §3.3` puts them here: a line of dialogue is one command, and what a script writes
//! inside that text is interpreted by whoever draws it. [`crate::text::place`] therefore reads the
//! runs (`vela_text::tags`) and draws each with its own emphasis.
//!
//! **Bold** is drawn twice, a hair apart — the long-standing way a renderer synthesizes a weight it
//! has no face for, and the honest one here: the project ships `LiberationSans-Regular` and
//! nothing else, so the alternative is to accept `{b}` and draw it plain. **Italic** *is* drawn
//! plain, because a shear is a transform the draw list does not have (M13's animation work), and
//! that gap is named rather than papered over with a fake slant.

use vela_text::TextEngine;

use crate::draw::{Color, DrawList, GlyphQuad};

/// How far apart the two passes of a synthetic bold are drawn, in pixels.
const BOLD_OFFSET: f32 = 1.0;

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

/// Lays out and emits one line of text, returning the height it occupied.
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
    let runs = vela_text::tags::runs(body);

    // The ordinary case, unchanged: one run with no emphasis is the paragraph path this always
    // took, and keeping it is what makes every existing golden frame byte-identical.
    if runs.len() == 1 && !runs[0].bold {
        return run(text, font, draw, &runs[0].text, at, color, false);
    }

    // Tagged: run by run, left to right. Each run wraps within what is left of the width, so a
    // line whose emphasis *crosses* a wrap point does not break as one paragraph would — the one
    // place this is not the same as shaping the whole line, and the reason rich inline runs want a
    // real layout pass rather than a loop.
    let mut left = at.left;
    let mut height: f32 = 0.0;

    for emphasis in &runs {
        let width = at.max_width - (left - at.left);
        if width <= 0.0 {
            break;
        }
        let placement = Placement { left, ..at };
        let occupied = run(
            text,
            font,
            draw,
            &emphasis.text,
            placement,
            color,
            emphasis.bold,
        );
        if occupied > 0.0 {
            left += advance(text, font, at.size, &emphasis.text, width);
            height = height.max(occupied);
        }
    }

    height
}

/// Draws one run with one emphasis, and returns the height it occupied.
fn run(
    text: &mut TextEngine,
    font: &str,
    draw: &mut DrawList,
    body: &str,
    at: Placement,
    color: Color,
    bold: bool,
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
            let quad = |x: f32| GlyphQuad {
                x,
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
            };
            let x = left + glyph.x + rect.left as f32;
            draw.push_glyph(quad(x));
            if bold {
                draw.push_glyph(quad(x + BOLD_OFFSET));
            }
        }
    }
    layout.height
}

/// How wide a run laid out in `max_width` is.
fn advance(text: &mut TextEngine, font: &str, size: f32, body: &str, max_width: f32) -> f32 {
    text.layout(font, size, body, Some(max_width))
        .map_or(0.0, |layout| layout.width)
}
