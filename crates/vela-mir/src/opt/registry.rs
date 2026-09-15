//! The pass registry.

use crate::opt::Pass;
use crate::opt::branch_simplify::BranchSimplify;
use crate::opt::cmd_fuse::CmdFuse;
use crate::opt::const_fold::ConstFold;
use crate::opt::dead_block::DeadBlock;
use crate::opt::inline_small::InlineSmall;

/// How hard to optimize.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub enum OptLevel {
    /// No passes at all. The reference form: what the author wrote, unaltered.
    #[default]
    None,
    /// Folding and cleanup. What a debug build uses.
    O1,
    /// Everything, including inlining.
    O2,
}

impl OptLevel {
    /// Whether a pass running at `pass` should run here.
    #[must_use]
    pub fn includes(self, pass: Self) -> bool {
        self >= pass
    }

    /// The level a `-O` spelling names.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "0" => Some(Self::None),
            "1" => Some(Self::O1),
            "2" => Some(Self::O2),
            _ => None,
        }
    }
}

/// The passes, in order.
///
/// Order matters, and the order in `BYTECODE.md §2.1` is not quite right. The spec lists
/// `dead_block` before `branch_simplify`, but folding a decided branch is exactly what
/// *creates* unreachable blocks — so running the cleanup first means the blocks it was
/// meant to remove are still there afterwards, and rule 8 of `BYTECODE.md §4` (unreachable
/// code must be removed, not merely present) fails on any story with a constant condition.
///
/// Hence: fold, then simplify what folding decided, then clean up what simplifying
/// orphaned, then inline, then fuse. The spec's table is corrected in M3's notes.
#[must_use]
pub fn passes() -> Vec<Box<dyn Pass>> {
    vec![
        Box::new(ConstFold),
        Box::new(BranchSimplify),
        Box::new(DeadBlock),
        Box::new(InlineSmall),
        Box::new(CmdFuse),
    ]
}

/// A pipeline as a value, so a caller can hold one rather than rebuild it.
pub struct Pipeline {
    level: OptLevel,
}

impl Pipeline {
    /// A pipeline at a level.
    #[must_use]
    pub fn at(level: OptLevel) -> Self {
        Self { level }
    }

    /// Runs it over a module.
    pub fn run(&self, module: &mut crate::ir::Module) {
        crate::opt::optimize(module, self.level);
    }
}
