//! Font loading and metrics.
//!
//! A font here is *bytes and an index*, and every question asked of it is answered by
//! reading the file. Nothing is cached at this level except the parsed face, because the
//! caches that matter — shaped runs, laid-out paragraphs, rasterised glyphs — are keyed by
//! the thing that changes (the text, the size) rather than by the thing that does not (the
//! font).

use swash::FontRef;
use swash::scale::Scaler;
use swash::scale::{Render, ScaleContext, Source};
use swash::shape::{ShapeContext, Shaper};

/// The glyph sources tried, in order.
///
/// Outlines only, for now: a bitmap strike or a colour layer would need a different texture
/// format and a different blend, and neither appears in the fixtures a VN ships. Listing
/// them as `Source`s rather than special-casing them is what makes adding them a one-line
/// change later.
const SOURCES: &[Source] = &[Source::Outline];

/// A loaded font face.
pub struct Font {
    data: Vec<u8>,
    index: u32,
    /// The face's own name, for diagnostics and for naming an atlas.
    name: String,
}

/// The vertical metrics of a font at a size, in pixels.
///
/// Baseline-relative, as a font file is: `ascent` is above the baseline and `descent` is
/// below it, so the distance between two baselines is their sum plus the leading, and a
/// line box is taller than the text in it — which is the thing every hand-rolled text
/// layout gets wrong first.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct FontMetrics {
    /// Distance from the baseline to the top of the tallest glyph.
    pub ascent: f32,
    /// Distance from the baseline to the bottom of the lowest glyph.
    pub descent: f32,
    /// Extra space a line box adds beyond ascent and descent.
    pub leading: f32,
}

impl FontMetrics {
    /// The distance between consecutive baselines.
    #[must_use]
    pub fn line_height(&self) -> f32 {
        self.ascent + self.descent + self.leading
    }
}

impl Font {
    /// Loads the face at `index` from an in-memory font file.
    ///
    /// Returns `None` for bytes that are not a font, because a corrupt asset is a *missing
    /// asset* to everything above this: there is no useful partial state, and a face with no
    /// glyphs would fail later and more confusingly.
    #[must_use]
    pub fn from_bytes(data: Vec<u8>, index: u32) -> Option<Self> {
        let reference = FontRef::from_index(&data, index as usize)?;
        let name = reference
            .localized_strings()
            .next()
            .map(|string| string.to_string())
            .unwrap_or_else(|| "<unnamed>".to_string());
        Some(Self { data, index, name })
    }

    /// The face's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// A borrowed view of the face.
    fn reference(&self) -> FontRef<'_> {
        FontRef::from_index(&self.data, self.index as usize).expect("validated at load")
    }

    /// The vertical metrics at `size`.
    #[must_use]
    pub fn metrics(&self, size: f32) -> FontMetrics {
        let scaled = self.reference().metrics(&[]).scale(size);
        FontMetrics {
            ascent: scaled.ascent,
            descent: scaled.descent,
            leading: scaled.leading,
        }
    }

    /// A shaper for this face at `size`.
    ///
    /// Returned rather than used internally so the caller owns the context: `ShapeContext`
    /// holds the caches, and a context rebuilt per string is a shaper that has forgotten
    /// everything it learned.
    #[must_use]
    pub fn shaper<'a>(context: &'a mut ShapeContext, font: &'a Self, size: f32) -> Shaper<'a> {
        context.builder(font.reference()).size(size).build()
    }

    /// A scaler for this face at `size`.
    #[must_use]
    pub fn scaler<'a>(context: &'a mut ScaleContext, font: &'a Self, size: f32) -> Scaler<'a> {
        context
            .builder(font.reference())
            .size(size)
            .hint(true)
            .build()
    }

    /// Rasterises one glyph at `size`.
    #[must_use]
    pub fn rasterize(scaler: &mut Scaler<'_>, id: u16) -> Option<swash::scale::image::Image> {
        Render::new(SOURCES).render(scaler, id)
    }
}
