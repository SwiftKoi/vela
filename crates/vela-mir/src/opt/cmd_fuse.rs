//! One yield site per command.
//!
//! `BYTECODE.md §2.1` asks for "adjacent `Cmd` construction merged with its `Yield`". At
//! MIR that is a *dead command elimination*: a block that builds two commands before
//! suspending hands the host only the second, because the first is overwritten. The first
//! one's arguments were still evaluated, so removing it is safe only because building a
//! command has no effect — which is what makes commands declarative rather than calls.

use crate::ir::{Module, StmtKind, Terminator};
use crate::opt::{OptLevel, Pass};

/// Drops command constructions that the next one overwrites.
pub struct CmdFuse;

impl Pass for CmdFuse {
    fn name(&self) -> &'static str {
        "cmd_fuse"
    }

    fn level(&self) -> OptLevel {
        OptLevel::O1
    }

    fn run(&self, module: &mut Module) {
        for body in module.fns.iter_mut().chain(module.labels.iter_mut()) {
            for block in &mut body.blocks {
                // Only a block that suspends has a command that is kept. Anywhere else the
                // last `Cmd` is itself dead, but proving that needs a reachability argument
                // this pass deliberately does not make.
                if !matches!(block.term, Terminator::Yield(_)) {
                    continue;
                }
                let last = block
                    .stmts
                    .iter()
                    .rposition(|stmt| matches!(stmt.kind, StmtKind::Cmd { .. }));
                if let Some(last) = last {
                    let mut index = 0;
                    block.stmts.retain(|stmt| {
                        let keep = index >= last || !matches!(stmt.kind, StmtKind::Cmd { .. });
                        index += 1;
                        keep
                    });
                }
            }
        }
    }
}
