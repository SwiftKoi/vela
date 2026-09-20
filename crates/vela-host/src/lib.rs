//! Adapter: the native window, the input table, the idle tick they are polled on, and the clock.
//!
//! # Owns
//!
//! The window backend, the `App` trait its consumer implements, the semantic actions input
//! resolves to, and the platform clock (`clock::stamp`) — the only wall time any crate in the
//! engine reads.
//!
//! # Does not own
//!
//! Anything a lower crate can do without a platform; the GPU (vela-render).

pub mod action;

pub use action::{Action, Bindings, Key};

pub mod clock;

pub use clock::stamp;

pub mod window;

pub use window::{App, Config, Window, run};
