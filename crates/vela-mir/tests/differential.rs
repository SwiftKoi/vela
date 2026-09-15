//! The differential harness.
//!
//! `BYTECODE.md §2.1` requires every optimization pass to be observably
//! semantics-preserving, and "observably" means: run the corpus unoptimized and optimized,
//! and assert the command streams and final worlds are identical.
//!
//! This is the net the whole back end hangs on. At M5 the same harness gains a second
//! executor — the real VM over bytecode — and becomes "the VM agrees with the reference
//! interpreter" as well as "the optimizer agrees with itself".

// Not `support/mod.rs`: a `mod.rs` is a facade by the project's own rule, and this
// holds real code. The path attribute keeps it out of that rule's way without moving it
// where cargo would try to build it as a test binary of its own.
#[path = "support/helpers.rs"]
mod support;

use std::path::Path;

use vela_mir::{Answer, Execution, OptLevel, Outcome, Terminator};

/// The label every corpus entry starts at.
const START: &str = "start";

#[test]
fn the_corpus_behaves_the_same_at_every_optimization_level() {
    // Two answer scripts, so that a menu's other arm is exercised as well as its first.
    // A run that needs more answers than it is given is acknowledged, so an entry with no
    // menu is unaffected by either.
    let scripts: [Vec<Answer>; 2] = [Vec::new(), vec![Answer::Choice(1)]];

    for input in support::corpus() {
        for answers in &scripts {
            let plain = run(&input, OptLevel::None, answers);
            let folded = run(&input, OptLevel::O2, answers);

            assert_eq!(
                plain.commands,
                folded.commands,
                "{}: the command stream changed under optimization",
                input.display()
            );
            assert_eq!(
                plain.world,
                folded.world,
                "{}: the final world changed under optimization",
                input.display()
            );
            assert_eq!(
                plain.outcome,
                folded.outcome,
                "{}: the outcome changed under optimization",
                input.display()
            );
        }
    }
}

#[test]
fn the_corpus_runs_to_completion() {
    for input in support::corpus() {
        let execution = run(&input, OptLevel::None, &[]);
        assert_eq!(
            execution.outcome,
            Outcome::Halted,
            "{}: did not run to completion",
            input.display()
        );
    }
}

/// One yield site per command is an invariant the pipeline has to *maintain*, not merely
/// achieve: lowering emits one `Cmd` per block by construction, so `cmd_fuse` has nothing
/// to do on lowering's output. What it is for is keeping that true if a later pass — today
/// only `inline_small`, which refuses to inline a body containing a command — ever could
/// break it.
#[test]
fn every_block_builds_at_most_one_command() {
    for input in support::corpus() {
        for level in [OptLevel::None, OptLevel::O2] {
            let module = lower(&input, level);
            for body in module.bodies() {
                for block in &body.blocks {
                    let commands = block
                        .stmts
                        .iter()
                        .filter(|stmt| matches!(stmt.kind, vela_mir::StmtKind::Cmd { .. }))
                        .count();
                    assert!(
                        commands <= 1,
                        "{}: `{}` b{} builds {commands} commands at {level:?}",
                        input.display(),
                        body.name,
                        block.id.0
                    );

                    // And a command is only ever built by a block that suspends, because
                    // otherwise nothing would ever hand it to the host.
                    if commands == 1 {
                        assert!(
                            matches!(block.term, Terminator::Yield(_)),
                            "{}: a command was built but never yielded",
                            input.display()
                        );
                    }
                }
            }
        }
    }
}

/// Runs one entry at a level.
fn run(path: &Path, level: OptLevel, answers: &[Answer]) -> Execution {
    Execution::run(&lower(path, level), START, answers)
}

/// Lowers one entry at a level.
fn lower(path: &Path, level: OptLevel) -> vela_mir::Module {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let name = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let mut sources = vela_span::SourceMap::new();
    let id = sources.add(name.as_str(), &text);
    let parsed = vela_syntax::parse(id, &text);
    let (env, _) = vela_types::Env::build(&parsed.program);
    let mut lowered = vela_mir::lower(
        &vela_hir::ModuleName::new(name.as_str()),
        &parsed.program,
        &env,
    );
    vela_mir::optimize(&mut lowered.module, level);
    lowered.module
}
