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

/// A rectangle a quad is drawn inside: what falls outside it is not drawn.
///
/// Pixels from the frame's top-left, as `[left, top, right, bottom]` — a corner pair rather than an
/// origin and a size, because clipping is intersection and two corners intersect with two `max` and two
/// `min`. A size would have to be recomputed from the other corner at every step, and a width that
/// went negative would mean something.
///
/// **Note.** The unbounded clip is two infinities, not the frame: a `DrawList` does not know how big
/// the frame is, and a quad that reaches past the edge of one is the window's business. A screen clips
/// to its own frame because the painter says so, not because the list assumed it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Clip {
    /// Left edge, in pixels.
    pub left: f32,
    /// Top edge.
    pub top: f32,
    /// Right edge.
    pub right: f32,
    /// Bottom edge.
    pub bottom: f32,
}

impl Clip {
    /// A clip that excludes nothing.
    pub const UNBOUNDED: Self = Self {
        left: f32::NEG_INFINITY,
        top: f32::NEG_INFINITY,
        right: f32::INFINITY,
        bottom: f32::INFINITY,
    };

    /// A clip of this rectangle.
    #[must_use]
    pub fn rect(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            left: x,
            top: y,
            right: x + width,
            bottom: y + height,
        }
    }

    /// This clip narrowed by another: a nested clip can only take pixels away.
    #[must_use]
    pub fn intersect(self, other: Self) -> Self {
        Self {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.min(other.right),
            bottom: self.bottom.min(other.bottom),
        }
    }

    /// Whether nothing can be drawn inside it.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.right <= self.left || self.bottom <= self.top
    }

    /// The rectangle as a vertex carries it: `[left, top, right, bottom]`.
    #[must_use]
    pub fn bounds(self) -> [f32; 4] {
        [self.left, self.top, self.right, self.bottom]
    }
}

impl Default for Clip {
    fn default() -> Self {
        Self::UNBOUNDED
    }
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

/// One step of a frame: a quad, or a change to what may be drawn.
///
/// The clip is an *operation* rather than a field on every quad, because a clip covers a region of the
/// frame's submission order and not one shape in it: a viewport opens a clip, draws whatever is inside
/// it — rectangles, glyphs, pictures, however many — and closes it. Storing it per quad would mean
/// every quad ever written carries a field that is unbounded for almost all of them, and every
/// constructor would have to say so.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Op {
    /// Narrow what may be drawn, until the matching [`Op::Unclip`].
    Clip(Clip),
    /// Close the innermost clip.
    Unclip,
    /// Draw this.
    Quad(Quad),
}

/// Everything a frame draws, in submission order.
///
/// One ordered sequence, not separate vectors for rectangles and glyphs. A frame is layered —
/// a pause menu is drawn *over* a dialogue — and a layer that cannot occlude what came before
/// it is not a layer. Ordering the quads here lets the renderer batch *runs* of consecutive
/// quads of one kind: the same number of draw calls as before for a frame that paints its
/// rectangles and then its text, and the correct picture for one that does not.
///
/// Clips are in the same sequence, so "draw this inside that" is a property of *where* a quad sits in
/// the order rather than of the quad itself.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct DrawList {
    ops: Vec<Op>,
    /// The clip in force: what a quad pushed now would be drawn inside.
    clip: Clip,
    /// The clips a push replaced, so a pop can restore one.
    ///
    /// The *outer* clips rather than the pushed ones, because restoring is what a pop does and the
    /// pushed clip is already in the ops for the vertex builder to read.
    enclosing: Vec<Clip>,
}

impl DrawList {
    /// An empty frame.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a filled rectangle.
    pub fn push_rect(&mut self, rect: RectQuad) {
        self.ops.push(Op::Quad(Quad::Rect(rect)));
    }

    /// Adds a glyph.
    pub fn push_glyph(&mut self, glyph: GlyphQuad) {
        self.ops.push(Op::Quad(Quad::Glyph(glyph)));
    }

    /// Adds an image.
    pub fn push_image(&mut self, image: ImageQuad) {
        self.ops.push(Op::Quad(Quad::Image(image)));
    }

    /// Narrows what may be drawn, until the matching [`DrawList::pop_clip`].
    ///
    /// Nested clips *intersect*, so a viewport inside a viewport draws only what both admit: a caller
    /// that pushed its clip is not unclipped by someone else's.
    pub fn push_clip(&mut self, clip: Clip) {
        let narrowed = self.clip.intersect(clip);
        self.ops.push(Op::Clip(narrowed));
        self.enclosing.push(self.clip);
        self.clip = narrowed;
    }

    /// Closes the innermost clip.
    ///
    /// A pop with nothing pushed is ignored rather than a panic: a draw list is built by walking a tree,
    /// and a walker that got that wrong should draw a wrong picture, not take the process down.
    pub fn pop_clip(&mut self) {
        self.ops.push(Op::Unclip);
        if let Some(outer) = self.enclosing.pop() {
            self.clip = outer;
        }
    }

    /// Every op, in submission order — the sound track of the frame, clips and all.
    #[must_use]
    pub fn ops(&self) -> &[Op] {
        &self.ops
    }

    /// Every quad, in submission order — a view, not a copy.
    pub fn quads(&self) -> impl Iterator<Item = &Quad> {
        self.ops.iter().filter_map(|op| match op {
            Op::Quad(quad) => Some(quad),
            Op::Clip(_) | Op::Unclip => None,
        })
    }

    /// The rectangles, in submission order — a view, not a copy.
    pub fn rects(&self) -> impl Iterator<Item = &RectQuad> {
        self.quads().filter_map(|quad| match quad {
            Quad::Rect(rect) => Some(rect),
            Quad::Glyph(_) | Quad::Image(_) => None,
        })
    }

    /// The glyphs, in submission order — a view, not a copy.
    pub fn glyphs(&self) -> impl Iterator<Item = &GlyphQuad> {
        self.quads().filter_map(|quad| match quad {
            Quad::Glyph(glyph) => Some(glyph),
            Quad::Rect(_) | Quad::Image(_) => None,
        })
    }

    /// The images, in submission order — a view, not a copy.
    pub fn images(&self) -> impl Iterator<Item = &ImageQuad> {
        self.quads().filter_map(|quad| match quad {
            Quad::Image(image) => Some(image),
            Quad::Rect(_) | Quad::Glyph(_) => None,
        })
    }

    /// How many quads there are in total.
    #[must_use]
    pub fn len(&self) -> usize {
        self.quads().count()
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

    /// How many images there are.
    #[must_use]
    pub fn image_count(&self) -> usize {
        self.images().count()
    }

    /// Whether nothing is drawn.
    ///
    /// A clip on its own is nothing drawn: a list that pushed one and stopped has an op and no pixels.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ops
            .iter()
            .all(|op| matches!(op, Op::Clip(_) | Op::Unclip))
    }
}
