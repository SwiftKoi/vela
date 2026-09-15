//! Text shaping, line breaking, layout, and the glyph atlas.
//!
//! # Owns
//!
//! ShapedRun, TextLayout, font subsetting hooks, the layout cache.
//!
//! # Does not own
//!
//! Drawing (vela-render); widget layout (vela-ui).
//!
//! # Shape of a frame
//!
//! ```text
//!   text ──shape──► ShapedRun ──break──► TextLayout ──rasterise──► GlyphRect + atlas
//!                        (one per paragraph)         (cached by content)
//! ```
//!
//! Shaping happens once per paragraph, not once per line: every candidate line break is a
//! prefix of the shaped run, so re-shaping per line would shape the same glyphs again. Layout
//! and rasterisation are both cached by everything that affects them, so an unchanged text
//! box costs nothing per frame (`ARCHITECTURE.md §8`).
//!
//! **Determinism.** Positions are `f32` and the arithmetic is `+`, `-`, `*` and `/`, which
//! IEEE-754 pins exactly; the shaper is deterministic for a given font and size. A layout
//! golden is therefore comparable byte for byte across machines, which is what makes it
//! usable as a CI artifact.

pub mod atlas;
pub mod engine;
pub mod font;
pub mod layout;
pub mod shape;

pub use atlas::{GlyphAtlas, GlyphRect};
pub use engine::TextEngine;
pub use font::{Font, FontMetrics};
pub use layout::{Line, PlacedGlyph, TextLayout};
pub use shape::{ShapedGlyph, ShapedRun};
