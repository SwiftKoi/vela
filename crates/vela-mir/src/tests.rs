//! Unit tests for lowering and the pass pipeline.
//!
//! Shape, not rendering: the golden corpus pins what MIR *looks like*, and these pin what it
//! *is*. A test here that asserted on printed text would have to be re-blessed every time
//! the printer was touched, which is how a golden stops being read.

use std::fs;
use std::path::Path;

use vela_diag::Diagnostic;
use vela_hir::ModuleName;
use vela_span::SourceMap;
use vela_types::Env;

use crate::{Const, Module, OptLevel, optimize};

/// Lowers one source text, and says what lowering reported.
fn lower(text: &str) -> (Module, Vec<Diagnostic>) {
    let mut sources = SourceMap::new();
    let id = sources.add("test", text);
    let parsed = vela_syntax::parse(id, text);
    let (env, _) = Env::build(&parsed.program);
    let lowered = crate::lower(&ModuleName::new("test"), &parsed.program, &env);
    (lowered.module, lowered.diagnostics)
}

/// The codes of a set of diagnostics.
fn codes(diagnostics: &[Diagnostic]) -> Vec<&str> {
    diagnostics.iter().map(|d| d.code.as_str()).collect()
}

#[test]
fn a_const_initialised_with_a_call_is_reported_e2004() {
    let (_, diagnostics) =
        lower("fn compute() -> int:\n    return 3\n\nconst LIMIT: int = compute()\n");
    assert_eq!(codes(&diagnostics), vec!["E2004"]);
}

#[test]
fn a_default_initialised_with_a_call_is_reported_e2004() {
    let (_, diagnostics) =
        lower("fn compute() -> int:\n    return 3\n\ndefault score: int = compute()\n");
    assert_eq!(codes(&diagnostics), vec!["E2004"]);
}

#[test]
fn a_constant_expression_is_folded_during_lowering() {
    let (module, diagnostics) = lower("const BASE: int = 2\nconst LIMIT: int = BASE * 3\n");
    assert!(diagnostics.is_empty(), "{:?}", codes(&diagnostics));

    let limit = module
        .consts
        .iter()
        .find(|definition| definition.name == "LIMIT")
        .expect("LIMIT was not lowered");
    assert_eq!(module.pool.get(limit.value), Some(&Const::Int(6)));
}

#[test]
fn string_interpolation_over_constants_folds_to_one_string() {
    let (module, _) = lower("const LIMIT: int = 6\nconst TEXT: str = \"limit [LIMIT]\"\n");
    let text = module
        .consts
        .iter()
        .find(|definition| definition.name == "TEXT")
        .expect("TEXT was not lowered");
    assert_eq!(
        module.pool.get(text.value),
        Some(&Const::Str("limit 6".to_string()))
    );
}

/// Adding a pass touches `opt/<name>.rs` and `opt/registry.rs` — and nothing else
/// (`CONVENTIONS.md §4.5`). These three tests are what makes that a fact: a pass with no
/// file, no registry entry, or no golden fails.
#[test]
fn every_pass_has_its_own_file() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/opt");
    for pass in crate::passes() {
        let file = directory.join(format!("{}.rs", pass.name()));
        assert!(
            file.is_file(),
            "`{}` has no `src/opt/{}.rs`",
            pass.name(),
            pass.name()
        );
    }
}

#[test]
fn every_pass_is_listed_in_the_registry() {
    let registry =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/opt/registry.rs"))
            .expect("read the registry");
    for pass in crate::passes() {
        assert!(
            registry.contains(pass.name()),
            "`{}` is not mentioned in opt/registry.rs",
            pass.name()
        );
    }
}

#[test]
fn every_pass_has_a_golden() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/mir");
    for pass in crate::passes() {
        let file = corpus.join(format!("pass_{}.vela", pass.name()));
        assert!(
            file.is_file(),
            "`{}` has no golden at {}",
            pass.name(),
            file.display()
        );
    }
}

#[test]
fn the_pipeline_runs_in_the_documented_order() {
    let names: Vec<&str> = crate::passes().iter().map(|pass| pass.name()).collect();
    assert_eq!(
        names,
        vec![
            "const_fold",
            "branch_simplify",
            "dead_block",
            "inline_small",
            "cmd_fuse"
        ]
    );
}

/// `-O0` is the reference form: what the author wrote, unaltered.
#[test]
fn nothing_runs_at_no_optimization() {
    let source = "label start:\n    var a = 1 + 2\n    return\n";
    let (plain, _) = lower(source);
    let (mut folded, _) = lower(source);
    optimize(&mut folded, OptLevel::None);

    // Compared as printed text:  has no equality of its own, and the printed form
    // is the thing the milestone's other criterion pins anyway.
    assert_eq!(crate::print_module(&plain), crate::print_module(&folded));
}

#[test]
fn a_block_after_a_return_is_removed() {
    let (mut module, _) = lower("label start:\n    return\n    \"never\"\n");
    let before = module.label_named("start").expect("start").blocks.len();
    optimize(&mut module, OptLevel::O2);
    let after = module.label_named("start").expect("start").blocks.len();

    assert!(
        before > after,
        "the unreachable block survived: {before} blocks"
    );
    assert_eq!(after, 1);
}

#[test]
fn a_small_function_is_inlined() {
    let (mut module, _) = lower(
        "fn ceiling() -> int:\n    return 10\n\nlabel start:\n    var value = ceiling()\n    return\n",
    );
    optimize(&mut module, OptLevel::O2);

    let start = module.label_named("start").expect("start");
    let calls = start
        .blocks
        .iter()
        .flat_map(|block| &block.stmts)
        .filter(|stmt| matches!(stmt.kind, crate::StmtKind::Call { .. }))
        .count();
    assert_eq!(calls, 0, "the call was not inlined");
}

#[test]
fn a_branch_on_a_constant_becomes_a_jump() {
    let (mut module, _) = lower("label start:\n    if false:\n        return\n    return\n");
    optimize(&mut module, OptLevel::O2);

    let start = module.label_named("start").expect("start");
    let branches = start
        .blocks
        .iter()
        .filter(|block| matches!(block.term, crate::Terminator::Branch { .. }))
        .count();
    assert_eq!(branches, 0, "the constant branch survived");
}
