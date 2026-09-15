//! Encoding a pack into its container.
//!
//! Writing cannot fail — a pack is plain data with no reference to the outside world — so there is
//! no `Result` here, exactly as in `vela-bytecode`'s `codec/write.rs`. The reader beside this
//! module is the half that can fail, and is written as though every byte came off a network.
//!
//! The byte sink is `codec`; the tree and expression writers live beside it, one function per kind
//! so a tag and its fields stay together.

mod codec;
mod expr;
mod tree;

pub(super) use codec::encode;
#[cfg(test)]
pub(super) use codec::encode_expr;
