//! Folding branches whose condition is already decided.
//!
//! A decided branch is the residue of `const_fold`, and removing it is what makes
//! unreachable-code diagnostics precise: the blocks a folded branch abandoned become
//! genuinely unreachable, so `dead_block` can delete them and the printer stops marking
//! them.

use crate::ir::{BlockId, Const, Module, Terminator, Value};
use crate::opt::{OptLevel, Pass};

/// Turns a branch on a constant into a jump.
pub struct BranchSimplify;

impl Pass for BranchSimplify {
    fn name(&self) -> &'static str {
        "branch_simplify"
    }

    fn level(&self) -> OptLevel {
        OptLevel::O1
    }

    fn run(&self, module: &mut Module) {
        let Module {
            fns, labels, pool, ..
        } = module;

        for body in fns.iter_mut().chain(labels.iter_mut()) {
            for block in &mut body.blocks {
                let Terminator::Branch { cond, then_, else_ } = &block.term else {
                    continue;
                };
                let taken: Option<BlockId> = match pool.get(match cond {
                    Value::Const(id) => *id,
                    Value::Slot(_) => continue,
                }) {
                    Some(Const::Bool(true)) => Some(*then_),
                    Some(Const::Bool(false)) => Some(*else_),
                    _ => None,
                };
                if let Some(taken) = taken {
                    block.term = Terminator::Goto(taken);
                }
            }
        }
    }
}
