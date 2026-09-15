//! Removing blocks nothing can reach.
//!
//! Small as a size win, but it is not optional: `BYTECODE.md §4` rule 8 requires
//! unreachable blocks to be *gone* by the time bytecode is verified, so that the
//! verifier's model stays total rather than needing a "this block was unreachable so its
//! stack depth does not have to add up" escape hatch.

use crate::ir::{Block, BlockId, Body, Module, Terminator, YieldSite};
use crate::opt::{OptLevel, Pass};

/// Deletes unreachable blocks and renumbers what is left.
pub struct DeadBlock;

impl Pass for DeadBlock {
    fn name(&self) -> &'static str {
        "dead_block"
    }

    fn level(&self) -> OptLevel {
        OptLevel::O1
    }

    fn run(&self, module: &mut Module) {
        for body in module.fns.iter_mut().chain(module.labels.iter_mut()) {
            remove_unreachable(body);
        }
    }
}

/// Deletes a body's unreachable blocks, renumbering the survivors.
fn remove_unreachable(body: &mut Body) {
    let dead = body.unreachable_blocks();
    if dead.is_empty() {
        return;
    }

    // Renumbering rather than leaving gaps keeps `BlockId` equal to its index, which the
    // verifier and the disassembler both rely on.
    let mut map: Vec<Option<BlockId>> = vec![None; body.blocks.len()];
    let mut next = 0u32;
    for (index, slot) in map.iter_mut().enumerate() {
        if !dead.contains(&BlockId(u32::try_from(index).unwrap_or(u32::MAX))) {
            *slot = Some(BlockId(next));
            next += 1;
        }
    }

    let blocks: Vec<Block> = body
        .blocks
        .iter()
        .filter(|block| !dead.contains(&block.id))
        .map(|block| Block {
            id: map[block.id.0 as usize].unwrap_or(block.id),
            stmts: block.stmts.clone(),
            term: remap(&block.term, &map),
        })
        .collect();

    body.entry = map[body.entry.0 as usize].unwrap_or(BlockId::ENTRY);
    body.blocks = blocks;
}

/// Rewrites a terminator's targets through the renumbering.
fn remap(term: &Terminator, map: &[Option<BlockId>]) -> Terminator {
    let to = |id: BlockId| map.get(id.0 as usize).copied().flatten().unwrap_or(id);

    match term {
        Terminator::Goto(target) => Terminator::Goto(to(*target)),
        Terminator::Branch { cond, then_, else_ } => Terminator::Branch {
            cond: *cond,
            then_: to(*then_),
            else_: to(*else_),
        },
        Terminator::CallLabel { target, ret } => Terminator::CallLabel {
            target: target.clone(),
            ret: to(*ret),
        },
        Terminator::Dispatch {
            enum_name,
            value,
            arms,
            else_,
        } => Terminator::Dispatch {
            enum_name: enum_name.clone(),
            value: *value,
            arms: arms
                .iter()
                .map(|(variant, block)| (*variant, to(*block)))
                .collect(),
            else_: to(*else_),
        },
        Terminator::Yield(site) => Terminator::Yield(YieldSite {
            resume: to(site.resume),
            ..*site
        }),
        Terminator::Return(_) | Terminator::JumpLabel(_) | Terminator::Unreachable => term.clone(),
    }
}
