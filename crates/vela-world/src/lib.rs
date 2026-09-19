//! Runtime state: typed values, entities, the deterministic RNG, and the serialization schema.
//!
//! # Owns
//!
//! `World`, `Value`, `Command`, schema derivation, pinned float formatting.
//!
//! # Does not own
//!
//! Snapshots and save files (vela-replay); the interpreter (vela-vm).
//!
//! # Why `Command` lives here
//!
//! A command is the vocabulary the runtime uses to describe presentation. It sits at rank
//! 1 rather than beside the VM because the *compiler* constructs these values when it
//! lowers `say` and `show`, so they have to be usable below rank 5 — and because a command
//! is close kin to the scene state it leaves behind. Both are presentation as plain data.

mod command;
mod input;
mod preferences;
mod rng;
mod stage;
mod value;
mod world;

#[cfg(test)]
mod tests;

pub use command::{Audio, Choice, Command, CommandKind, Stage};
pub use input::Input;
pub use preferences::Preferences;
pub use rng::Rng;
pub use stage::{AudioState, Playing, SceneState, StagedImage};
pub use value::{Key, Value, format_float};
pub use world::{Tick, World};
