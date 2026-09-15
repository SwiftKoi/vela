//! Terminators: how a block ends.
//!
//! Story control flow stays *first class* here. `JumpLabel` and `CallLabel` are not
//! desugared into raw jumps at this layer, because the story graph has to remain
//! recoverable from MIR — that is what lets `vela analyze` answer questions about a story
//! without re-parsing it, and what keeps the label graph visible in a disassembly.

use vela_world::CommandKind;

use crate::ir::body::BlockId;
use crate::ir::defs::{Slot, VariantId};
use crate::ir::stmt::Value;

/// A label, resolved to a module and a name.
///
/// Resolved, not written: `forest.clearing` and an aliased `f.clearing` name the same
/// target, and collapsing them here is what keeps the runtime free of any notion of
/// modules or imports.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LabelRef {
    /// The module the label is in, when it is not this one.
    pub module: Option<String>,
    /// The label's own name.
    pub label: String,
}

/// A suspension point.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct YieldSite {
    /// Which command is being presented.
    pub command: CommandKind,
    /// Where execution resumes once the host acknowledges.
    pub resume: BlockId,
    /// Where a command that *returns* a value leaves it — a menu's chosen index.
    pub result: Option<Slot>,
}

/// How a block ends.
#[derive(Clone, PartialEq, Debug)]
pub enum Terminator {
    /// Unconditional transfer.
    Goto(BlockId),
    /// Two-way branch on a condition.
    Branch {
        /// The condition.
        cond: Value,
        /// Taken when it holds.
        then_: BlockId,
        /// Taken when it does not.
        else_: BlockId,
    },
    /// Leave the body, with a value if it has one.
    Return(Option<Value>),
    /// Transfer to a label, never returning.
    JumpLabel(LabelRef),
    /// Call a label, resuming at `ret` when it returns.
    CallLabel {
        /// Where being called.
        target: LabelRef,
        /// Where to resume.
        ret: BlockId,
    },
    /// Branch on an enum's tag over every variant.
    Dispatch {
        /// Which enum is being matched, so that a [`VariantId`] can be turned back into a
        /// name. Without it the table is unreadable and rule 6 of `BYTECODE.md §4` — that
        /// it covers the enum's declared variant range — cannot be checked by anyone.
        enum_name: String,
        /// The value being matched.
        value: Value,
        /// One target per variant, indexed by [`VariantId`] rather than searched.
        arms: Vec<(VariantId, BlockId)>,
        /// Where the catch-all arm goes, if there is one.
        else_: BlockId,
    },
    /// Suspend and hand the host a command.
    Yield(YieldSite),
    /// No way in, and none out.
    Unreachable,
}

impl Terminator {
    /// Every block this one can transfer to.
    #[must_use]
    pub fn successors(&self) -> Vec<BlockId> {
        match self {
            Self::Goto(target) => vec![*target],
            Self::Branch { then_, else_, .. } => vec![*then_, *else_],
            Self::CallLabel { ret, .. } => vec![*ret],
            Self::Dispatch { arms, else_, .. } => {
                let mut targets: Vec<BlockId> = arms.iter().map(|(_, target)| *target).collect();
                targets.push(*else_);
                targets
            }
            Self::Yield(site) => vec![site.resume],
            Self::Return(_) | Self::JumpLabel(_) | Self::Unreachable => Vec::new(),
        }
    }

    /// Whether control can leave the body here.
    #[must_use]
    pub fn is_exit(&self) -> bool {
        matches!(self, Self::Return(_) | Self::JumpLabel(_))
    }
}
