//! Widget registration: the declared surface of every widget a screen can name.
//!
//! This module is a facade. The registry lives in `registry.rs`, the prop declarations in
//! `schema.rs`, and the default set in `builtin.rs`.

pub mod builtin;
pub mod registry;
pub mod schema;

pub use registry::{Category, Widget, WidgetRegistry};
pub use schema::{PropDecl, PropType, PropsSchema};
