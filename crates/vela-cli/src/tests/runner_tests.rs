//! `vela test`: a suite runs headless, and a story that does not do what it says fails.
//!
//! The exit code is the verdict here, which is what the milestone's criterion is about — "runs a suite
//! headless in CI and fails a deliberately broken story". The tests therefore go through the command
//! line rather than through `vela-test`'s API: the crate has its own tests for what a failure *is*, and
//! these are about a project being green or red when a build server asks.

use super::support::{cli, temp_project};

/// A story with a menu, and a test that picks one arm of it.
const SUITE: &str = "\
default trust: int = 0

label start:
    \"It rained all evening.\"
    menu \"Which way?\":
        \"Inside\":
            trust = trust + 1
            jump ending
        \"Outside\":
            jump ending

label ending:
    \"That was the night.\"
    return

test \"going inside earns trust\":
    run
    advance 1
    choose \"Inside\"
    expect trust == 1

test \"and the ending is reached\":
    run
    choose \"Outside\"
    cover labels
";

/// The exit code and output of `vela test`.
fn test_project(name: &str, source: &str) -> (u8, String) {
    let project = temp_project(name, source);
    cli(&["test", &project.display().to_string()])
}

#[test]
fn a_suite_that_holds_is_green() {
    let (code, output) = test_project("suite-green", SUITE);

    assert_eq!(code, 0, "{output}");
    assert!(output.contains("ok   going inside earns trust"), "{output}");
    assert!(output.contains("2 passed, 0 failed"), "{output}");
}

/// The criterion, in one test: a story that does not do what its test says fails the build.
///
/// Three ways to be broken, and each is a different mistake: an assertion that is false, a choice that
/// is not offered, and a label the run never reaches. A runner that only caught the first would pass two
/// broken stories.
#[test]
fn a_broken_story_fails() {
    let (code, output) = test_project(
        "suite-assertion",
        &SUITE.replace("expect trust == 1", "expect trust == 99"),
    );
    assert_ne!(code, 0, "{output}");
    assert!(output.contains("FAIL going inside earns trust"), "{output}");
    assert!(output.contains("`trust == 99` is not true"), "{output}");
    // The failure points at the line in the test, which is what makes it fixable.
    assert!(output.contains("main.vela:20:"), "{output}");

    let (code, output) = test_project(
        "suite-choice",
        &SUITE.replace("choose \"Inside\"", "choose \"Sideways\""),
    );
    assert_ne!(code, 0, "{output}");
    assert!(
        output.contains("no option reads `Sideways`; the story offers `Inside`, `Outside`"),
        "{output}"
    );

    let (code, output) = test_project(
        "suite-cover",
        &SUITE.replace(
            "    choose \"Outside\"\n    cover labels\n",
            "    cover labels\n",
        ),
    );
    assert_ne!(code, 0, "{output}");
    assert!(
        output.contains("the story offers a choice and the script does not say which"),
        "a `cover` drives the story to its end, and the menu on the way had no answer: {output}"
    );
}

/// Coverage is about the run that happened: a label on no path the test takes is a failure that names
/// it, which is the point of asking.
#[test]
fn cover_labels_names_a_label_the_run_never_reached() {
    let (code, output) = test_project(
        "suite-uncovered",
        "\
label start:
    \"One.\"
    return

label orphan:
    \"Nobody says this.\"
    return

test \"everything is reached\":
    run
    cover labels
",
    );

    assert_ne!(code, 0, "{output}");
    assert!(
        output.contains("the run never reached `main.orphan`"),
        "{output}"
    );
}

/// A project with no tests is not a failure: most projects have none yet, and a build server asking
/// "did the tests pass" of a project with no tests is asking a question with the answer "yes".
#[test]
fn a_project_without_tests_is_green() {
    let (code, output) = test_project("suite-none", "label start:\n    \"Hello.\"\n    return\n");

    assert_eq!(code, 0, "{output}");
    assert!(output.contains("no tests"), "{output}");
}

/// `--filter` runs a subset, which is what makes a suite tolerable while writing one.
#[test]
fn filter_runs_one_test() {
    let project = temp_project("suite-filter", SUITE);
    let (code, output) = cli(&["test", &project.display().to_string(), "--filter", "ending"]);

    assert_eq!(code, 0, "{output}");
    assert!(output.contains("and the ending is reached"), "{output}");
    assert!(
        !output.contains("going inside earns trust"),
        "the other test did not run: {output}"
    );
    assert!(output.contains("1 passed, 0 failed"), "{output}");
}

/// A directive the runner cannot honour is reported rather than skipped, and it does not fail the
/// build: the test said something this version does not do, which is a gap and not a mistake in the
/// story.
#[test]
fn an_unsupported_directive_is_a_note() {
    let (code, output) = test_project(
        "suite-note",
        &SUITE.replace("    cover labels\n", "    cover variants\n"),
    );

    assert_eq!(code, 0, "{output}");
    assert!(
        output.contains("note: `cover variants` is not implemented"),
        "{output}"
    );
}
