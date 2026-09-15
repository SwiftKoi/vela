//! The draw list: what a frame is made of, with no GPU anywhere near it.
//!
//! A `DrawList` is the renderer's input and the draw stages' only vocabulary. Keeping it
//! plain data means the mapping from story commands to pixels can be tested without a
//! device, which matters because a GPU test that fails says "the pixels differ" and a
//! draw-list test that fails says *which rectangle* is in the wrong place.

/// A straight colour, in linear space.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Color {
    /// Red.
    pub r: f32,
    /// Green.
    pub g: f32,
    /// Blue.
    pub b: f32,
    /// Alpha.
    pub a: f32,
}

impl Color {
    /// An opaque colour from 8-bit components, which is how themes are written.
    #[must_use]
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self {
            r: f32::from(r) / 255.0,
            g: f32::from(g) / 255.0,
            b: f32::from(b) / 255.0,
            a: 1.0,
        }
    }

    /// Transparent, for a stage that has nothing to say.
    pub const TRANSPARENT: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };
}

/// An axis-aligned filled rectangle.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RectQuad {
    /// Left edge, in pixels, from the top-left of the frame.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
    /// Fill.
    pub color: Color,
}

impl RectQuad {
    /// A rectangle with corners at two points.
    #[must_use]
    pub fn from_corners(left: f32, top: f32, right: f32, bottom: f32, color: Color) -> Self {
        Self {
            x: left,
            y: top,
            width: (right - left).max(0.0),
            height: (bottom - top).max(0.0),
            color,
        }
    }
}

/// A glyph, textured from the atlas.
///
/// Positions are in pixels, and `uv` is in atlas texture coordinates — normalised, because
/// that is what a sampler takes, and converting here rather than in the shader keeps the
/// shader from needing to know the atlas size.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct GlyphQuad {
    /// Left edge, in pixels.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
    /// Atlas coordinates, as `[left, top, right, bottom]` normalised.
    pub uv: [f32; 4],
    /// Tint.
    pub color: Color,
}

/// An image, textured from one of the uploaded images.
///
/// The third case beside a filled rectangle and a glyph, and for a visual novel the *first*
/// one: a background is the content of a scene, not a decoration on it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ImageQuad {
    /// Left edge, in pixels.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
    /// Image coordinates, as `[left, top, right, bottom]` normalised.
    pub uv: [f32; 4],
    /// Which uploaded image it samples.
    pub image: u32,
    /// Tint, multiplied into the texel — white for "draw it as it is".
    pub color: Color,
}

/// Which texture a quad samples.
///
/// What the renderer batches by. Three sources rather than two is the whole reason this is an
/// enum and not the `is_glyph` boolean it started as: runs of consecutive quads from *one*
/// source share a bind group, and runs from different sources do not.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    /// The one-texel white texture: a filled rectangle.
    White,
    /// The glyph atlas.
    Atlas,
    /// One of the uploaded images.
    Image(u32),
}

/// One thing a frame draws, in the order it was submitted.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Quad {
    /// A filled rectangle, textured from the white texel.
    Rect(RectQuad),
    /// A glyph, textured from the atlas.
    Glyph(GlyphQuad),
    /// An image, textured from itself.
    Image(ImageQuad),
}

impl Quad {
    /// Whether this quad samples the atlas rather than the white texel.
    #[must_use]
    pub fn is_glyph(&self) -> bool {
        matches!(self, Self::Glyph(_))
    }

    /// What this quad samples, which is what a draw call is chosen by.
    #[must_use]
    pub fn source(&self) -> Source {
        match self {
            Self::Rect(_) => Source::White,
            Self::Glyph(_) => Source::Atlas,
            Self::Image(image) => Source::Image(image.image),
        }
    }
}

/// Everything a frame draws, in submission order.
///
/// One ordered sequence, not separate vectors for rectangles and glyphs. A frame is layered —
/// a pause menu is drawn *over* a dialogue — and a layer that cannot occlude what came before
/// it is not a layer. Ordering the quads here lets the renderer batch *runs* of consecutive
/// quads of one kind: the same number of draw calls as before for a frame that paints its
/// rectangles and then its text, and the correct picture for one that does not.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct DrawList {
    quads: Vec<Quad>,
}

impl DrawList {
    /// An empty frame.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a filled rectangle.
    pub fn push_rect(&mut self, rect: RectQuad) {
        self.quads.push(Quad::Rect(rect));
    }

    /// Adds a glyph.
    pub fn push_glyph(&mut self, glyph: GlyphQuad) {
        self.quads.push(Quad::Glyph(glyph));
    }

    /// Adds an image.
    pub fn push_image(&mut self, image: ImageQuad) {
        self.quads.push(Quad::Image(image));
    }

    /// Every quad, in submission order.
    #[must_use]
    pub fn quads(&self) -> &[Quad] {
        &self.quads
    }

    /// The rectangles, in submission order — a view, not a copy.
    pub fn rects(&self) -> impl Iterator<Item = &RectQuad> {
        self.quads.iter().filter_map(|quad| match quad {
            Quad::Rect(rect) => Some(rect),
            Quad::Glyph(_) | Quad::Image(_) => None,
        })
    }

    /// The glyphs, in submission order — a view, not a copy.
    pub fn glyphs(&self) -> impl Iterator<Item = &GlyphQuad> {
        self.quads.iter().filter_map(|quad| match quad {
            Quad::Glyph(glyph) => Some(glyph),
            Quad::Rect(_) | Quad::Image(_) => None,
        })
    }

    /// The images, in submission order — a view, not a copy.
    pub fn images(&self) -> impl Iterator<Item = &ImageQuad> {
        self.quads.iter().filter_map(|quad| match quad {
            Quad::Image(image) => Some(image),
            Quad::Rect(_) | Quad::Glyph(_) => None,
        })
    }

    /// How many quads there are in total.
    #[must_use]
    pub fn len(&self) -> usize {
        self.quads.len()
    }

    /// How many rectangles there are.
    #[must_use]
    pub fn rect_count(&self) -> usize {
        self.rects().count()
    }

    /// How many glyphs there are.
    #[must_use]
    pub fn glyph_count(&self) -> usize {
        self.glyphs().count()
    }

    /// Whether nothing is drawn.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.quads.is_empty()
    }
}
