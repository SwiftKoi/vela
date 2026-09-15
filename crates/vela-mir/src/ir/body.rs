//! Bodies: a name, a frame, and a control-flow graph.

use std::fmt;

use vela_types::Ty;

use crate::ir::defs::{LocalDecl, Slot};
use crate::ir::stmt::Stmt;
use crate::ir::term::Terminator;

/// An index into a body's blocks.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct BlockId(pub u32);

impl BlockId {
    /// The block a body starts at.
    pub const ENTRY: Self = Self(0);
}

/// A name.
///
/// A newtype rather than a bare `String` so that interning can land without touching every
/// signature that mentions a name. Interning is a *codegen* concern — the constant pool and
/// the string table are built there — so MIR carries names as they were written.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Symbol(pub String);

impl Symbol {
    /// The name, as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A basic block: straight-line statements and one way out.
#[derive(Clone, Debug)]
pub struct Block {
    /// The block's index, equal to its position in the body.
    pub id: BlockId,
    /// The statements, in order.
    pub stmts: Vec<Stmt>,
    /// How control leaves.
    pub term: Terminator,
}

/// A function or label, as a control-flow graph.
#[derive(Clone, Debug)]
pub struct Body {
    /// The body's name: the function's or label's identifier.
    pub name: Symbol,
    /// The slots holding arguments, in declaration order.
    pub params: Vec<Slot>,
    /// What the body produces. A label returns nothing; a `fn` may return a value.
    pub ret: Ty,
    /// Every slot, indexed by slot number.
    pub locals: Vec<LocalDecl>,
    /// The blocks, indexed by block number.
    pub blocks: Vec<Block>,
    /// Where execution starts.
    pub entry: BlockId,
}

impl Body {
    /// The block with this index.
    #[must_use]
    pub fn block(&self, id: BlockId) -> Option<&Block> {
        self.blocks.get(id.0 as usize)
    }

    /// The declaration of a slot.
    #[must_use]
    pub fn local(&self, slot: Slot) -> Option<&LocalDecl> {
        self.locals.get(slot.0 as usize)
    }

    /// The type a slot holds.
    #[must_use]
    pub fn slot_ty(&self, slot: Slot) -> Option<&Ty> {
        self.local(slot).map(|decl| &decl.ty)
    }

    /// Every block, with its index.
    pub fn blocks(&self) -> impl Iterator<Item = BlockId> + use<> {
        (0..self.blocks.len())
            .filter_map(|index| u32::try_from(index).ok())
            .map(BlockId)
    }

    /// Blocks no terminator can reach, in index order.
    ///
    /// Used by the printer to mark them and by `dead_block` to remove them. Reachability
    /// starts at `entry`, so a block that is only *written to* is still unreachable.
    #[must_use]
    pub fn unreachable_blocks(&self) -> Vec<BlockId> {
        let mut seen = vec![false; self.blocks.len()];
        let mut pending = vec![self.entry];

        while let Some(id) = pending.pop() {
            let Some(slot) = seen.get_mut(id.0 as usize) else {
                continue;
            };
            if *slot {
                continue;
            }
            *slot = true;

            let Some(block) = self.block(id) else {
                continue;
            };
            for successor in block.term.successors() {
                pending.push(successor);
            }
        }

        self.blocks()
            .filter(|id| !seen.get(id.0 as usize).copied().unwrap_or(false))
            .collect()
    }
}
