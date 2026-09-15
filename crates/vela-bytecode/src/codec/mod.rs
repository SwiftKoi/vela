//! The `.velac` container.
//!
//! `BYTECODE.md §3.1`: little-endian, fixed-width, no alignment padding. Every section is
//! length-prefixed so a loader can skip one it does not understand, which is what makes an
//! additive format change something an old reader survives.
//!
//! Encoding and decoding live in separate files because they fail in different ways.
//! Writing cannot fail. Reading can, in every direction, and the reader is written as though
//! every byte came off a network — because it did.

mod commands;
mod hash;
mod read;
mod tables;
mod write;

pub use commands::known_commands;
pub(crate) use hash::fnv1a;
pub use read::{DecodeError, decode};
pub use write::encode;
