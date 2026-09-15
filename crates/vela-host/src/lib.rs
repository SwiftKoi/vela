//! Adapter: platform traits and their native implementations - window, input, filesystem, clock.
//!
//! # Owns
//!
//! The Host/Clock/Input/Fs traits, the native backend, the save directory.
//!
//! # Does not own
//!
//! Anything a lower crate can do without a platform; the GPU (vela-render).

pub mod action;

pub use action::{Action, Bindings, Key};

pub mod window;

pub use window::{App, Config, Window, run};
