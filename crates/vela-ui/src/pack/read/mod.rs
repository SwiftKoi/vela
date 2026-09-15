//! Decoding a pack from its container.
//!
//! Total, in the sense that matters for a loader: no input makes this panic. A truncated file, a
//! bad magic number, a checksum that does not match, or a length that runs past the end all
//! produce an error — because the input is a file that arrived beside a build, and a loader that
//! crashes on a corrupt one is a loader that crashes.
//!
//! Every read is bounds-checked once, in `cursor`, rather than at each call site. Beyond this point
//! a pack is the same in-memory shape `vela build` compiled, so nothing downstream knows a
//! container was involved.

mod cursor;
mod expr;
mod tree;

pub(super) use cursor::decode;
#[cfg(test)]
pub(super) use cursor::decode_expr;
