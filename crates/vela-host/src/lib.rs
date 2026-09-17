//! Adapter: the native window, the input table, and the idle tick they are polled on.
//!
//! # Owns
//!
//! The window backend, the `App` trait its consumer implements, and the semantic actions input
//! resolves to.
//!
//! # Does not own
//!
//! Anything a lower crate can do without a platform; the GPU (vela-render).

pub mod action;

pub use action::{Action, Bindings, Key};

pub mod window;

pub use window::{App, Config, Window, run};
