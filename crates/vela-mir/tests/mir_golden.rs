//! Golden tests for the MIR pretty-printer.
//!
//! The goldens pin *rendering*: block numbering, which slots are named and which are
//! temporaries, how a terminator reads. Lowering's own behaviour is pinned by the unit
//! tests in the crate, which assert on shapes. Keeping them apart is what makes a golden
//! diff readable — a rendering change touches every file, a lowering change touches one.
//!
//! `cargo xtask bless` regenerates every `.expected`. CI never blesses.

// Not `support/mod.rs`: a `mod.rs` is a facade by the project's own rule, and this
// holds real code. The path attribute keeps it out of that rule's way without moving it
// where cargo would try to build it as a test binary of its own.
#[path = "support/helpers.rs"]
mod support;

use std::fs;

use vela_mir::{StmtKind, Terminator, print_module};

use support::{blessing, corpus, lower_entry};

#[test]
fn goldens_match() {
    let mut differences = Vec::new();

    for input in corpus() {
        let lowered = lower_entry(&input);
        let mut actual = print_module(&lowered.module);
        for diagnostic in &lowered.diagnostics {
            actual.push_str(&vela_diag::render(diagnostic, &lowered.sources));
        }

        let expected_path = input.with_extension("expected");
        if blessing() {
            fs::write(&expected_path, &actual).expect("write golden");
            continue;
        }

        let expected = fs::read_to_string(&expected_path).unwrap_or_default();
        if actual != expected {
            differences.push(format!(
                "{}\n--- expected ---\n{expected}--- actual ---\n{actual}",
                input.display()
            ));
        }
    }

    assert!(
        differences.is_empty(),
        "{} golden(s) differ — run `cargo xtask bless`, then review the diff\n\n{}",
        differences.len(),
        differences.join("\n")
    );
}

/// Every MIR form is produced by something in the corpus.
///
/// This is what turns "every construct in `LANGUAGE.md §3` lowers" from a claim into a
/// check. Each match below is exhaustive, so adding a form to the IR without adding
/// coverage for it fails to compile rather than passing silently.
#[test]
fn every_mir_form_appears_in_the_corpus() {
    let mut statements: Vec<&'static str> = Vec::new();
    let mut terminators: Vec<&'static str> = Vec::new();

    for input in corpus() {
        let lowered = lower_entry(&input);
        for body in lowered.module.bodies() {
            for block in &body.blocks {
                for stmt in &block.stmts {
                    statements.push(statement_form(&stmt.kind));
                }
                terminators.push(terminator_form(&block.term));
            }
        }
    }

    let missing = |present: &[&str], forms: &[&'static str]| -> Vec<String> {
        forms
            .iter()
            .filter(|form| !present.contains(form))
            .map(ToString::to_string)
            .collect()
    };

    let all_statements = ALL_STATEMENTS.to_vec();
    let all_terminators = ALL_TERMINATORS.to_vec();

    let missing_statements = missing(&statements, &all_statements);
    let missing_terminators = missing(&terminators, &all_terminators);

    assert!(
        missing_statements.is_empty(),
        "no corpus entry produces: {}",
        missing_statements.join(", ")
    );
    assert!(
        missing_terminators.is_empty(),
        "no corpus entry produces: {}",
        missing_terminators.join(", ")
    );
}

/// Every statement form, so that adding one is a compile error until it is covered.
const ALL_STATEMENTS: [&str; 13] = [
    "assign",
    "assign_unary",
    "load",
    "call",
    "cmd",
    "list",
    "map",
    "struct",
    "variant",
    "enum_field",
    "is_none",
    "unwrap",
    "len",
];

/// Every terminator form.
const ALL_TERMINATORS: [&str; 8] = [
    "goto",
    "branch",
    "return",
    "jump_label",
    "call_label",
    "dispatch",
    "yield",
    "unreachable",
];

/// The name of a statement's form.
fn statement_form(kind: &StmtKind) -> &'static str {
    match kind {
        StmtKind::Assign { .. } => "assign",
        StmtKind::AssignUn { .. } => "assign_unary",
        StmtKind::Load { src, .. } => match src {
            vela_mir::Operand::Len { .. } => "len",
            _ => "load",
        },
        StmtKind::Call { .. } => "call",
        StmtKind::Cmd { .. } => "cmd",
        StmtKind::ListNew { .. } => "list",
        StmtKind::MapNew { .. } => "map",
        StmtKind::StructNew { .. } => "struct",
        StmtKind::EnumNew { .. } => "variant",
        StmtKind::EnumField { .. } => "enum_field",
        StmtKind::IsNone { .. } => "is_none",
        StmtKind::Unwrap { .. } => "unwrap",
    }
}

/// The name of a terminator's form.
fn terminator_form(term: &Terminator) -> &'static str {
    match term {
        Terminator::Goto(_) => "goto",
        Terminator::Branch { .. } => "branch",
        Terminator::Return(_) => "return",
        Terminator::JumpLabel(_) => "jump_label",
        Terminator::CallLabel { .. } => "call_label",
        Terminator::Dispatch { .. } => "dispatch",
        Terminator::Yield(_) => "yield",
        Terminator::Unreachable => "unreachable",
    }
}
