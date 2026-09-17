//! Adapter: the wgpu renderer, render graph, and per-target shader backends.
//!
//! # Owns
//!
//! Renderer, RenderGraph, draw lists, shader backend selection.
//!
//! # Does not own
//!
//! Text layout (vela-text); widget trees (vela-ui).
//!
//! # One renderer, two targets
//!
//! ```text
//!   DrawList ──► RenderGraph ──► Renderer ──┬── a surface  (a window, on a desktop)
//!                                           └── a texture  (a PNG, in CI and in tests)
//! ```
//!
//! The window is a *target*, not an assumption. That is forced by the exit criterion:
//! *"text layout golden: byte-identically across the CI matrix"* — CI has no display, so a
//! renderer that only knows how to draw into a window cannot be verified there, and a golden
//! that only runs on a developer's machine is not a golden.
//!
//! # Extension
//!
//! A new render stage is a type implementing [`Stage`]. The graph is a `Vec` of them and
//! knows nothing about what any of them do (`ARCHITECTURE.md §5`, row 6) — `tests/graph.rs`
//! inserts a stage it defines itself and asserts the order, which is the check that the
//! extension point still exists.

pub mod capture;
pub mod draw;
pub mod geometry;
pub mod graph;
pub mod menu;
pub mod pipeline;
pub mod present;
pub mod renderer;
pub mod stages;
pub mod surface;
pub mod text;
pub mod texture;

pub use capture::{CAPTURE_FORMAT, Capture};
pub use draw::{Clip, Color, DrawList, GlyphQuad, ImageQuad, Op, Quad, RectQuad};
pub use geometry::build_vertices;
pub use graph::{Frame, RenderGraph, Stage};
pub use menu::Menu;
pub use present::{Presenter, Style, tint_of};
pub use renderer::{ATLAS_FORMAT, Renderer, Vertex};
pub use stages::{ClearStage, GeometryStage};
pub use surface::{Presented, Surface};
