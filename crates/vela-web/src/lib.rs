//! The browser entry point: the engine's VM, exposed to JavaScript.
//!
//! # Owns
//!
//! The wasm module a browser loads, and the `Player` a page drives.
//!
//! # Does not own
//!
//! The interpreter (vela-vm); the state model (vela-world); rendering, which is the next thing
//! this needs and is not here yet.
//!
//! # Why this is a crate of its own
//!
//! `BUILD_AND_ASSETS.md §5` makes web a first-class target, and a target needs a *driver*: the
//! thing that speaks the platform's language. JavaScript's language is `wasm-bindgen`'s, and
//! keeping it in one adapter crate is what stops its marshalling from spreading into the engine.
//! It is also why this crate needs no `unsafe` of its own — the raw pointers a wasm ABI is made
//! of live in that dependency, where they are audited — so the workspace's
//! `unsafe_code = "forbid"` stays as written.
//!
//! A facade: the `Player` lives beside this in `player.rs`.
//!
//! # Nothing here runs on a native target
//!
//! The whole crate is `wasm32`-only, so `cargo build --workspace` on a developer's machine
//! builds it to an empty library. That is not a stub: a browser entry point has no meaning off a
//! browser, and a version that compiled elsewhere would be one more thing to keep working for no
//! one.
//!
//! # What this is not yet
//!
//! It runs a story and hands back the commands. It does **not** draw: a playable page needs the
//! renderer on `wgpu`'s web backends and a canvas, and until that exists a `web` bundle plays in
//! *text*. Saying so here rather than in a pull-request description is the difference between a
//! limitation and a surprise.

#![cfg(target_arch = "wasm32")]

mod player;

pub use player::Player;
