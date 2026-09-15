//! Line breaking and line boxes.

use unicode_linebreak::{BreakOpportunity, linebreaks};

use crate::font::{Font, FontMetrics};
use crate::shape::{self, ShapedRun};

/// A glyph placed on a line.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PlacedGlyph {
    /// The glyph id in the font.
    pub id: u16,
    /// Left edge, relative to the line's own origin.
    pub x: f32,
    /// The glyph's baseline is the line's baseline, and the renderer positions from there.
    pub advance: f32,
}

/// One laid-out line.
#[derive(Clone, PartialEq, Debug)]
pub struct Line {
    /// The glyphs on this line, positioned.
    pub glyphs: Vec<PlacedGlyph>,
    /// The byte range of the source this line covers, which is what a caret needs.
    pub range: std::ops::Range<u32>,
    /// The line's width.
    pub width: f32,
    /// The baseline, measured from the top of the text box.
    pub baseline: f32,
}

/// A laid-out paragraph.
#[derive(Clone, PartialEq, Debug)]
pub struct TextLayout {
    /// The lines, top to bottom.
    pub lines: Vec<Line>,
    /// The widest line.
    pub width: f32,
    /// The total height, which is the number of line boxes, not of baselines.
    pub height: f32,
    /// The metrics every line was placed with.
    pub metrics: FontMetrics,
}

/// Lays out `text`, breaking lines to fit `max_width`.
///
/// Greedy, not optimal: `Knuth`-style breaking minimises raggedness, and a visual novel's
/// text box is wide and shallow, so the difference is a pixel and the cost is a pass over
/// every combination. `max_width` of `None` means no wrapping, and a single line.
#[must_use]
pub fn layout(
    font: &Font,
    size: f32,
    text: &str,
    max_width: Option<f32>,
    context: &mut swash::shape::ShapeContext,
) -> TextLayout {
    let metrics = font.metrics(size);
    // Shaped once for the whole paragraph: every candidate break is a prefix of this run, so
    // re-shaping per line would repeat the same work per line.
    let run = shape::shape(font, size, text, context);

    let breaks = match max_width {
        Some(limit) => wrap(text, &run, limit),
        None => vec![u32::try_from(text.len()).unwrap_or(u32::MAX)],
    };

    let mut lines = Vec::with_capacity(breaks.len());
    let start_of_line = 0;
    for (index, end) in breaks.iter().enumerate() {
        let start = if index == 0 {
            start_of_line
        } else {
            breaks[index - 1]
        };
        let glyphs = glyphs_in(&run, start, *end);
        let width = glyphs.iter().map(|glyph| glyph.advance).sum();
        lines.push(Line {
            glyphs,
            range: start..*end,
            width,
            baseline: metrics.ascent + (index as f32 * metrics.line_height()),
        });
    }

    let width = lines
        .iter()
        .fold(0.0f32, |widest, line| widest.max(line.width));
    let height = lines.len() as f32 * metrics.line_height();
    TextLayout {
        lines,
        width,
        height,
        metrics,
    }
}

/// The glyphs whose cluster falls in `start..end`, re-based to the line's origin.
fn glyphs_in(run: &ShapedRun, start: u32, end: u32) -> Vec<PlacedGlyph> {
    let mut placed = Vec::new();
    let mut pen = 0.0;
    for glyph in &run.glyphs {
        if glyph.cluster < start {
            continue;
        }
        if glyph.cluster >= end {
            break;
        }
        placed.push(PlacedGlyph {
            id: glyph.id,
            x: pen + glyph.x,
            advance: glyph.advance,
        });
        pen += glyph.advance;
    }
    placed
}

/// Greedy line breaking: the latest allowed break that still fits.
///
/// Break opportunities come from `unicode_linebreak`, which implements UAX #14 — so this
/// does not need to know that a break is allowed after a hyphen but not after a non-breaking
/// space, or that CJK breaks between characters. A word longer than the line is placed
/// anyway rather than overflowed into an empty line, because an unbreakable word that does
/// not fit has to go somewhere and a line is better than a loop.
fn wrap(text: &str, run: &ShapedRun, max_width: f32) -> Vec<u32> {
    let mut ends = Vec::new();
    let mut line_start = 0u32;
    let mut last_fit: Option<u32> = None;
    // The previous opportunity, which is where the current *word* begins. Needed because
    // when nothing fits, the break belongs at the end of that word rather than at the next
    // opportunity — otherwise a word too long for the line shares it with the next one.
    let mut word_start: Option<u32> = None;

    for (offset, opportunity) in linebreaks(text) {
        let offset = u32::try_from(offset).unwrap_or(u32::MAX);
        let width = run.advance_before(offset) - run.advance_before(line_start);

        // Break only where the line would actually overflow. Waiting for the *next*
        // opportunity would be a bug, not a simplification: if the remaining text fits the
        // width, no opportunity ever overflows, and the whole tail is appended as one line.
        if width > max_width && offset > line_start {
            let end = last_fit
                .filter(|end| *end > line_start)
                .or_else(|| word_start.filter(|end| *end > line_start))
                .unwrap_or(offset);
            ends.push(end);
            line_start = end;
            last_fit = None;
            // The segment that just overflowed starts the new line. It may *still* be too
            // long, in which case it is one unbreakable word and takes the line alone —
            // which is why this re-measures rather than assuming it now fits.
            if run.advance_before(offset) - run.advance_before(line_start) <= max_width {
                last_fit = Some(offset);
            }
        } else {
            last_fit = Some(offset);
        }
        word_start = Some(offset);

        // A mandatory break ends the line wherever it currently ends, fitting or not: it is
        // what a newline in the source means. Skipping these was the original bug — the
        // end-of-paragraph break is `Mandatory`, so the final line was never checked at all.
        if matches!(opportunity, BreakOpportunity::Mandatory) && offset > line_start {
            if ends.last().copied() != Some(offset) {
                ends.push(offset);
            }
            line_start = offset;
            last_fit = None;
        }
    }

    let total = u32::try_from(text.len()).unwrap_or(u32::MAX);
    if ends.last().copied() != Some(total) {
        ends.push(total);
    }
    ends
}
