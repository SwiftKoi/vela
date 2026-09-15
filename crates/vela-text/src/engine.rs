//! The engine: fonts, contexts, the layout cache, and the atlas.
//!
//! This is where `ARCHITECTURE.md §8`'s instruction — *cached and invalidated by change, not
//! recomputed per frame* — is actually carried out. Both caches here are keyed by everything
//! that affects the answer, so a stale entry is not something to invalidate politely: it
//! cannot exist. Changing the text, the size, the font, or the width produces a different
//! key and a different entry.

use std::collections::BTreeMap;
use std::rc::Rc;

use swash::scale::ScaleContext;
use swash::shape::ShapeContext;

use crate::atlas::{GlyphAtlas, GlyphRect};
use crate::font::{Font, FontMetrics};
use crate::layout::{self, TextLayout};

/// What a laid-out paragraph is cached by.
///
/// Every input to the layout, so an entry cannot be wrong. The text is cloned rather than
/// borrowed because a cache that borrows its keys borrows its callers.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct LayoutKey {
    font: String,
    size: u32,
    text: String,
    /// Width quantised to whole pixels: sub-pixel wrap differences are invisible and would
    /// make the cache useless by giving every animated width its own entry.
    max_width: Option<u32>,
}

/// The presentation-side text state.
pub struct TextEngine {
    fonts: BTreeMap<String, Rc<Font>>,
    shape: ShapeContext,
    scale: ScaleContext,
    atlas: GlyphAtlas,
    layouts: BTreeMap<LayoutKey, Rc<TextLayout>>,
}

impl Default for TextEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TextEngine {
    /// Creates an engine with an empty atlas.
    #[must_use]
    pub fn new() -> Self {
        Self {
            fonts: BTreeMap::new(),
            shape: ShapeContext::new(),
            scale: ScaleContext::new(),
            atlas: GlyphAtlas::new(1024, 1024),
            layouts: BTreeMap::new(),
        }
    }

    /// Registers a font under a name.
    pub fn add_font(&mut self, name: &str, font: Font) {
        self.fonts.insert(name.to_string(), Rc::new(font));
    }

    /// A registered font.
    #[must_use]
    pub fn font(&self, name: &str) -> Option<Rc<Font>> {
        self.fonts.get(name).cloned()
    }

    /// The metrics of a registered font at a size.
    #[must_use]
    pub fn metrics(&self, font: &str, size: f32) -> Option<FontMetrics> {
        self.fonts.get(font).map(|font| font.metrics(size))
    }

    /// Lays out a paragraph, or returns the cached layout of an identical one.
    ///
    /// Returns `None` for an unknown font, because there is no sensible empty layout: a
    /// caller that asked for a font it did not register has a bug, and silently returning no
    /// text would hide it.
    pub fn layout(
        &mut self,
        font: &str,
        size: f32,
        text: &str,
        max_width: Option<f32>,
    ) -> Option<Rc<TextLayout>> {
        let key = LayoutKey {
            font: font.to_string(),
            size: size.to_bits(),
            text: text.to_string(),
            max_width: max_width.map(|width| width.max(0.0) as u32),
        };
        if let Some(cached) = self.layouts.get(&key) {
            return Some(cached.clone());
        }

        let face = self.fonts.get(font)?.clone();
        let computed = Rc::new(layout::layout(
            &face,
            size,
            text,
            max_width,
            &mut self.shape,
        ));
        self.layouts.insert(key, computed.clone());
        Some(computed)
    }

    /// The atlas rectangle for a glyph, rasterising it on first use.
    pub fn glyph(&mut self, font: &str, size: f32, id: u16) -> Option<GlyphRect> {
        let face = self.fonts.get(font)?.clone();
        self.atlas.glyph(&face, size, id, &mut self.scale)
    }

    /// The atlas, for a caller that has to upload it.
    #[must_use]
    pub fn atlas(&self) -> &GlyphAtlas {
        &self.atlas
    }

    /// The atlas mutably, for draining its dirty rectangles.
    pub fn atlas_mut(&mut self) -> &mut GlyphAtlas {
        &mut self.atlas
    }

    /// How many layouts are cached.
    #[must_use]
    pub fn cached_layouts(&self) -> usize {
        self.layouts.len()
    }
}
