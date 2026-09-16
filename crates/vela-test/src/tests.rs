//! Running tests: what passes, and what a failure looks like when it does not.
//!
//! Every fixture here goes through the whole front end — parse, check, lower, compile, verify — because
//! that is what the runner does: the assertions it reads are functions in the same module as the story,
//! compiled by the same compiler. A fixture that only parsed would test a runner nobody will run.

use vela_bytecode::Module;
use vela_diag::Severity;
use vela_span::FileId;

use crate::plan::{Plan, StepKind, prepare};
use crate::report::Failure;
use crate::run::run;

/// A story with a menu, an ending, and one label nothing reaches.
const STORY: &str = "\
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
fn module(text: &str) -> Module {
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
fn suite(text: &str) -> (Module, Vec<Plan>) {
    let prepared = prepare(FileId::from_raw(0), text);
    (module(&prepared.text), prepared.plans)
}

/// The story with one test appended.
fn with_test(directives: &str) -> String {
    format!("{STORY}\ntest \"a test\":\n{directives}")
}

/// Runs one test and returns its failures.
fn failures(directives: &str) -> Vec<Failure> {
    let (module, plans) = suite(&with_test(directives));
    assert_eq!(plans.len(), 1, "one test");
    let report = run(&module, "start", &plans);
    report
        .outcomes
        .into_iter()
        .next()
        .expect("one outcome")
        .failures
}

/// Answers with `advance`, then asserts what the run left behind.
#[test]
fn a_scripted_run_that_holds_passes() {
    assert!(
        failures("    run from start\n    advance 1\n    expect trust == 0\n").is_empty(),
        "a story that does what it says"
    );
}

/// The assertion is evaluated in the world as it is at that point in the run, which is the whole reason
/// it is compiled rather than interpreted from the tree.
#[test]
fn an_assertion_sees_the_world_where_it_stands() {
    assert!(
        failures(
            "    run from menued\n    choose \"Left\"\n    expect trust == 1\n    expect trust > 0\n"
        )
        .is_empty()
    );
}

/// A choice is matched by the text the player reads, and the story takes that branch.
#[test]
fn choosing_by_text_takes_that_branch() {
    let empty = failures("    run from menued\n    choose \"Right\"\n    expect trust == 0\n");
    assert!(empty.is_empty(), "the other branch does not set `trust`");

    let wrong = failures("    run from menued\n    choose \"Left\"\n    expect trust == 0\n");
    assert_eq!(wrong.len(), 1, "{wrong:?}");
    assert!(matches!(wrong[0], Failure::Assertion { .. }), "{wrong:?}");
}

/// A choice nobody offers is a failure that says what was on offer, which is the message a person needs:
/// the script and the story stopped agreeing somewhere, and this is where.
#[test]
fn choosing_something_that_is_not_there_names_the_options() {
    let failures = failures("    run from menued\n    choose \"Nowhere\"\n");
    assert_eq!(failures.len(), 1, "{failures:?}");

    let Failure::NoSuchChoice {
        wanted, offered, ..
    } = &failures[0]
    else {
        panic!("expected a choice failure, got {failures:?}");
    };
    assert_eq!(wanted, "Nowhere");
    assert_eq!(offered, &vec!["Left".to_string(), "Right".to_string()]);
    assert!(
        failures[0].message().contains("`Left`"),
        "{}",
        failures[0].message()
    );
}

/// An `advance` that runs into a question fails rather than answering it: a script that never mentions a
/// menu either does not know about it or is testing a path that does not go through it.
#[test]
fn an_advance_that_meets_a_choice_says_so() {
    let failures = failures("    run from menued\n    advance 1\n");

    assert_eq!(failures.len(), 1, "{failures:?}");
    let Failure::UnscriptedChoice { offered, .. } = &failures[0] else {
        panic!("expected an unscripted choice, got {failures:?}");
    };
    assert_eq!(offered.len(), 2);
}

/// A false assertion says which one, in the author's own words.
#[test]
fn a_false_assertion_quotes_the_assertion() {
    let failures = failures("    run from start\n    advance 1\n    expect trust == 5\n");

    assert_eq!(failures.len(), 1, "{failures:?}");
    let Failure::Assertion { source, .. } = &failures[0] else {
        panic!("expected an assertion failure, got {failures:?}");
    };
    assert_eq!(source, "trust == 5");
    assert!(failures[0].message().contains("trust == 5"));
}

/// `cover labels` is a claim about the whole run, so a run to a label that is not there fails and names
/// what was missed.
#[test]
fn cover_labels_reports_the_labels_the_run_never_reached() {
    let failures = failures("    run from menued\n    choose \"Left\"\n    cover labels\n");

    assert_eq!(failures.len(), 1, "{failures:?}");
    let Failure::Uncovered { labels, .. } = &failures[0] else {
        panic!("expected an uncovered failure, got {failures:?}");
    };
    assert_eq!(
        labels,
        &vec!["extra".to_string(), "start".to_string()],
        "sorted, and both of them"
    );
}

/// And a run that goes through everything passes, which is what says the check above is about the run
/// rather than about coverage being impossible.
#[test]
fn cover_labels_passes_when_the_run_reaches_everything() {
    // The whole label goes, not just its body: `extra` is the label nothing reaches, and a coverage
    // claim about a story that has one is a claim that fails.
    let text = STORY.replace("label extra:\n    \"Nobody comes here.\"\n    return\n", "");
    let text = text.replace(
        "label start:\n    \"One.\"\n    jump menued",
        "label start:\n    jump menued",
    );
    // `extra` is gone, so every remaining label is on the one path from the entry.
    let text =
        format!("{text}\ntest \"all of it\":\n    run\n    choose \"Right\"\n    cover labels\n");

    let (module, plans) = suite(&text);
    let report = run(&module, "start", &plans);

    assert!(report.is_ok(), "{:?}", report.outcomes[0].failures);
}

/// A suite is a list of tests, and the report counts them the way a person would.
#[test]
fn a_suite_reports_each_test() {
    let text = format!(
        "{STORY}\n\
         test \"holds\":\n    run from start\n    advance 1\n    expect trust == 0\n\n\
         test \"does not\":\n    run from start\n    advance 1\n    expect trust == 9\n"
    );
    let (module, plans) = suite(&text);
    let report = run(&module, "start", &plans);

    assert_eq!(report.outcomes.len(), 2);
    assert_eq!(report.passed(), 1);
    assert_eq!(report.failed(), 1);
    assert!(!report.is_ok());
    assert_eq!(report.outcomes[0].name, "holds");
    assert_eq!(report.outcomes[1].name, "does not");
}

/// A file with no tests is left exactly as it was: most files are stories, and a runner that appended
/// something to every one of them would be a compiler that compiles a different program than `vela run`.
#[test]
fn a_file_without_tests_is_untouched() {
    let prepared = prepare(FileId::from_raw(0), "label start:\n    return\n");

    assert!(prepared.plans.is_empty());
    assert_eq!(prepared.text, "label start:\n    return\n");
}

/// What the runner cannot do is reported rather than skipped: a directive that silently does nothing is
/// a test that checks less than it says.
#[test]
fn a_directive_that_cannot_be_honoured_is_reported() {
    let (_, plans) = suite(&with_test("    run from start\n    cover variants\n"));

    assert_eq!(plans.len(), 1);
    assert!(
        plans[0].steps.is_empty(),
        "nothing to run: {:?}",
        plans[0].steps
    );
    assert_eq!(plans[0].notes.len(), 1);
    assert!(
        plans[0].notes[0].contains("cover variants"),
        "{:?}",
        plans[0].notes
    );
}

/// The steps a test declares are read in order, and the ones that carry an expression carry a function
/// name rather than the expression — because the expression is compiled.
#[test]
fn the_steps_are_read_in_order() {
    let (_, plans) = suite(&with_test(
        "    run from menued\n    choose \"Left\"\n    advance 2\n    expect trust == 1\n",
    ));

    let kinds: Vec<&StepKind> = plans[0].steps.iter().map(|step| &step.kind).collect();
    assert_eq!(
        kinds.len(),
        3,
        "one step per directive that runs: {kinds:?}"
    );
    assert!(matches!(kinds[0], StepKind::Choose { source, .. } if source == "\"Left\""));
    assert!(matches!(kinds[1], StepKind::Advance(2)));
    assert!(matches!(kinds[2], StepKind::Expect { source, .. } if source == "trust == 1"));
    assert_eq!(plans[0].start.as_deref(), Some(&["menued".to_string()][..]));
}

/// An assertion written after the story is over is read after it is over.
///
/// `expect` in the tail — the example project writes them below a `cover`, which drives the run to its
/// end — is an assertion about the finished game: the world is final and the machine has no frames left.
#[test]
fn an_assertion_is_read_after_the_story_ends() {
    let text = "\
default trust: int = 0

label start:
    trust = trust + 2
    return

test \"after it is over\":
    run
    cover labels
    expect trust == 2
";
    let (module, plans) = suite(text);
    let report = run(&module, "start", &plans);

    assert!(report.is_ok(), "{:?}", report.outcomes[0].failures);
}
