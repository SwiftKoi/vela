//! The screen runtime: widget registry, layout solver, styling, and accessibility.
//!
//! # Owns
//!
//! Tree, Widget trait, WidgetRegistry, styles, actions, hot reload.
//!
//! # Does not own
//!
//! Drawing (vela-render); text layout (vela-text).
//!
//! # Shape of a layout
//!
//! ```text
//!   Node tree ──measure──► desired sizes ──arrange──► Frame tree (rects)
//! ```
//!
//! Two passes, because one cannot do it: a parent cannot divide space before it knows what
//! its children want, and a child cannot know what to want before it is told what is left
//! (`SCREENS.md §4.1`).
//!
//! The solver is **pure** — a tree in, rectangles out — and that is the point. Layout is the
//! part of a UI most often adjusted by eye against a running window, and a pure function gives
//! what a screenshot cannot: an answer a test can assert on, with no GPU and no display.

pub mod a11y;
pub mod actions;
pub mod cache;
pub mod check;
pub mod compose;
pub mod deps;
pub mod error;
pub mod eval;
pub mod focus;
pub mod instantiate;
pub mod layout;
pub mod pack;
pub mod paint;
pub mod props;
pub mod reload;
pub mod screens;
pub mod styles;
pub mod theme;
pub mod tokens;
pub mod tree;
pub mod widgets;

pub use a11y::{A11yNode, Role, check_labels, tree};
pub use actions::{Action, ActionDecl, ActionRegistry};
pub use cache::{Cached, ScreenCache};
pub use check::check_screen;
pub use deps::{DepSet, deps_of};
pub use error::PackError;
pub use eval::{Args, Value};
pub use focus::{Hotspot, hotspots};
pub use layout::{Constraints, Frame, Rect, layout};
pub use pack::{PACK_VERSION, PackedSet, ScreenPack};
pub use paint::paint;
pub use props::{Anchor, Props, SizeSpec};
pub use reload::{Diff, Key, diff};
pub use screens::{Laid, ScreenSet};
pub use styles::{check_inheritance, check_screen_styles, resolve};
pub use theme::{Palette, Rgb, check_contrast, palette};
pub use tokens::check_magic_colours;
pub use tree::{Kind, Node, Paint, Size};
pub use widgets::{Category, Widget, WidgetRegistry};
