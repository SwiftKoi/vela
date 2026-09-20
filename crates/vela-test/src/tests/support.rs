//! The fixtures the runner's test modules share.
//!
//! Every fixture goes through the whole front end — parse, check, lower, compile, verify — because that
//! is what the runner does: the expressions it reads are functions in the same module as the story,
//! compiled by the same compiler. A fixture that only parsed would test a runner nobody will run.

use vela_bytecode::Module;
use vela_diag::Severity;
use vela_span::FileId;

use crate::plan::{Plan, prepare};
use crate::report::Failure;
use crate::run::run;

/// A story with a menu, an ending, and one label nothing reaches.
pub(super) const STORY: &str = "\
default trust: int = 0

label start:
    \"One.\"
    jump menued

label menued:
    menu \"Which way?\":
        \"Left\":
            trust = trust + 1
            jump ending
        \"Right\":
            jump ending

label ending:
    \"Done.\"
    return

label extra:
    \"Nobody comes here.\"
    return
";

/// Compiles a fixture through the front end, asserting every stage is clean.
pub(super) fn module(text: &str) -> Module {
    let file = FileId::from_raw(0);
    let parsed = vela_syntax::parse(file, text);
    assert!(
        parsed.diagnostics.is_empty(),
        "the fixture does not parse: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| format!("{}: {}", d.code.as_str(), d.message))
            .collect::<Vec<_>>()
    );

    let (env, _) = vela_types::Env::build(&parsed.program);
    let checked = vela_types::check(&parsed.program, &env);
    assert!(
        checked.iter().all(|d| d.severity() != Severity::Error),
        "the fixture does not check: {:?}",
        checked
            .iter()
            .map(|d| format!("{}: {}", d.code.as_str(), d.message))
            .collect::<Vec<_>>()
    );

    let lowered = vela_mir::lower(&vela_hir::ModuleName::new("main"), &parsed.program, &env);
    assert!(
        lowered.diagnostics.is_empty(),
        "the fixture does not lower: {:?}",
        lowered
            .diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect::<Vec<_>>()
    );

    let module = vela_bytecode::compile(&lowered.module, true);
    assert!(
        vela_bytecode::verify(&module).is_empty(),
        "the fixture does not verify"
    );
    module
}

/// Prepares and compiles a suite: the story, plus the functions the tests' expressions need.
pub(super) fn suite(text: &str) -> (Module, Vec<Plan>) {
    let prepared = prepare(FileId::from_raw(0), text);
    (module(&prepared.text), prepared.plans)
}

/// The story with one test appended.
pub(super) fn with_test(directives: &str) -> String {
    format!("{STORY}\ntest \"a test\":\n{directives}")
}

/// Runs one test and returns its failures.
pub(super) fn failures(directives: &str) -> Vec<Failure> {
    let (module, plans) = suite(&with_test(directives));
    assert_eq!(plans.len(), 1, "one test");
    let report = run(&module, "start", &plans, None, None);
    report
        .outcomes
        .into_iter()
        .next()
        .expect("one outcome")
        .failures
}
