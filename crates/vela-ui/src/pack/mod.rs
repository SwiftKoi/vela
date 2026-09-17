//! The screen pack: a project's interface, compiled (`SCREENS.md §13`).
//!
//! Running a *project* parses a screen's declaration every time it starts. A **bundle** must not:
//! `BUILD_AND_ASSETS.md §1` and `RUNTIME.md §8` make "nothing is compiled at run time" the whole
//! point of a built artifact, and that has to hold for the interface as much as for the story. So
//! `vela build` compiles each module's screens once — parse, check, build the [`ScreenSet`]'s
//! inputs — and writes the result here. A run from a bundle decodes a pack and has no parser, no
//! checker, and no `.vela` file in the path to drawing a screen.
//!
//! # The container
//!
//! Binary, little-endian, and shaped exactly like `.velac` (`vela-bytecode`'s codec), because the
//! two are the same kind of thing and should not look like two different ideas:
//!
//! ```text
//!   magic     [u8; 4]   b"VELS"
//!   version   u16       PACK_VERSION
//!   flags     u32       reserved; written zero
//!   sections  each u32 length-prefixed, in fixed order:
//!               module    string
//!               screens   count + ScreenDecl
//!               styles    count + StyleDecl
//!               palette   count + (token, r, g, b)
//!               fonts     count + (token, name)
//!   checksum  u64       FNV-1a over every byte before it
//! ```
//!
//! The reader is total: no input makes it panic, a length is checked against what is left before
//! anything is allocated, and a version it does not know is refused rather than read as though its
//! unknown fields were absent. The checksum is not a signature — it catches the two ways a pack
//! actually arrives broken, a truncated download and a half-written file.
//!
//! [`ScreenSet`]: crate::screens::ScreenSet

mod model;
mod read;
mod write;

#[cfg(test)]
mod tests;

pub use model::{MAGIC, PACK_VERSION, PackedSet, ScreenPack};
