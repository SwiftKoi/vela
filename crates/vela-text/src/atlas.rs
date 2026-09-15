//! The glyph atlas: rasterised glyphs, packed, and the record of what changed.
//!
//! The atlas is a CPU-side cache of alpha masks. `vela-render` owns the *texture*; this owns
//! what belongs in it, because rasterising a glyph is a font question and uploading one is a
//! GPU question, and `vela-text` is rank 1 — it has no GPU to speak to.
//!
//! **Invalidation is by change, not by frame** (`ARCHITECTURE.md §8`). A glyph already in
//! the atlas is never re-rasterised, and the caller is told exactly which rectangles are new
//! so it can upload a sub-rectangle instead of the whole image. A text box that does not
//! change costs nothing per frame, which is the entire point.

use std::collections::BTreeMap;

use swash::scale::ScaleContext;

use crate::font::Font;

/// What a glyph is cached by: the glyph id, at an exact size.
///
/// The size is keyed by its bits rather than by a rounded value, so two sizes that differ do
/// not silently share a rasterisation. Sizes come from a theme, so there are a handful, and
/// exactness costs nothing.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct GlyphKey {
    id: u16,
    size: u32,
}

/// Where a glyph's mask lives in the atlas, and how to place it on the baseline.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GlyphRect {
    /// Left edge in the atlas.
    pub x: u32,
    /// Top edge in the atlas.
    pub y: u32,
    /// Width of the mask.
    pub width: u32,
    /// Height of the mask.
    pub height: u32,
    /// Horizontal offset from the pen to the mask's left edge.
    pub left: i32,
    /// Vertical offset from the baseline to the mask's *top* edge, upwards positive — the
    /// convention font files use and screen coordinates do not.
    pub top: i32,
}

/// One row of packed glyphs.
struct Shelf {
    /// The top of the shelf.
    y: u32,
    /// The height of the tallest glyph on it.
    height: u32,
    /// How much of the shelf has been handed out.
    used: u32,
}

/// A packed collection of glyph masks.
pub struct GlyphAtlas {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    rects: BTreeMap<GlyphKey, GlyphRect>,
    shelves: Vec<Shelf>,
    /// Rectangles filled since the last `take_dirty`, for a partial upload.
    dirty: Vec<GlyphRect>,
}

impl GlyphAtlas {
    /// Creates an empty atlas.
    ///
    /// 1024² is 1 MiB of alpha, which holds a few thousand glyphs at text sizes — more than
    /// a visual novel's script uses, and one texture.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; (width * height) as usize],
            rects: BTreeMap::new(),
            shelves: Vec::new(),
            dirty: Vec::new(),
        }
    }

    /// The atlas image, one byte of coverage per pixel.
    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// The atlas dimensions.
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// How many glyphs are cached.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rects.len()
    }

    /// Whether nothing has been rasterised yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }

    /// The rectangles filled since this was last called.
    ///
    /// Draining rather than peeking: an upload is a transfer, and a caller that uploads is a
    /// caller that has consumed the news. Returning them twice would upload twice.
    pub fn take_dirty(&mut self) -> Vec<GlyphRect> {
        std::mem::take(&mut self.dirty)
    }

    /// The rectangle for a glyph, rasterising it if this is the first time it is asked for.
    ///
    /// `None` means the glyph has no mask — a space, or a glyph the font draws as nothing —
    /// or the atlas is full. Recording that absence is deliberate: a space is asked for on
    /// every line of every frame, and re-rasterising it each time is exactly the per-frame
    /// cost the cache exists to avoid.
    pub fn glyph(
        &mut self,
        font: &Font,
        size: f32,
        id: u16,
        context: &mut ScaleContext,
    ) -> Option<GlyphRect> {
        let key = GlyphKey {
            id,
            size: size.to_bits(),
        };
        if let Some(rect) = self.rects.get(&key) {
            return Some(*rect);
        }

        let mut scaler = Font::scaler(context, font, size);
        let image = Font::rasterize(&mut scaler, id)?;
        let width = image.placement.width;
        let height = image.placement.height;

        // A glyph with no pixels is still a cache hit next time, and still advances the pen.
        if width == 0 || height == 0 {
            return None;
        }

        let (x, y) = self.allocate(width, height)?;
        for row in 0..height {
            let source = (row * width) as usize;
            let target = ((y + row) * self.width + x) as usize;
            self.pixels[target..target + width as usize]
                .copy_from_slice(&image.data[source..source + width as usize]);
        }

        let rect = GlyphRect {
            x,
            y,
            width,
            height,
            left: image.placement.left,
            top: image.placement.top,
        };
        self.rects.insert(key, rect);
        self.dirty.push(rect);
        Some(rect)
    }

    /// Finds room for a glyph, opening a shelf if the current ones do not fit.
    fn allocate(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        if width > self.width || height > self.height {
            return None;
        }

        if let Some(shelf) = self.shelves.last_mut() {
            if shelf.used + width <= self.width && height <= shelf.height.max(height) {
                let x = shelf.used;
                let y = shelf.y;
                shelf.used += width;
                // A taller glyph grows the shelf, and only downwards: the glyphs already on
                // it were placed against this y, so they cannot move.
                shelf.height = shelf.height.max(height);
                return Some((x, y));
            }
        }

        let y = self
            .shelves
            .last()
            .map_or(0, |shelf| shelf.y + shelf.height);
        if y + height > self.height {
            return None;
        }
        self.shelves.push(Shelf {
            y,
            height,
            used: width,
        });
        Some((0, y))
    }
}
