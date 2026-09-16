//! `vela analyze`: the report's shape, and its manners.
//!
//! The JSON is `crates/vela-cli/tests/analyze_golden.rs`, which is where determinism gets checked against
//! a committed file. These are the rest: the two other formats, and what the command says when it cannot
//! do what it was asked.

use super::support::{cli, temp_project};

/// A story with a label nothing jumps to.
const STORY: &str = "\
label start:
    \"It begins.\"
    jump ending

label ending:
    \"It ends.\"
    return

label orphan:
    \"Nobody comes here.\"
    return
";

#[test]
fn the_text_report_marks_what_a_run_cannot_reach() {
    let project = temp_project("analyze-text", STORY);
    let (code, output) = cli(&["analyze", &project.display().to_string()]);

    assert_eq!(code, 0, "{output}");
    assert!(output.contains("! main.orphan"), "{output}");
    assert!(output.contains("  main.start"), "{output}");
    assert!(output.contains("-> main.ending"), "{output}");
    assert!(
        output.contains("3 labels, 1 edge, 2 reached, 1 unreachable, 2 terminal"),
        "{output}"
    );
}

#[test]
fn the_dot_report_dashes_what_a_run_cannot_reach() {
    let project = temp_project("analyze-dot", STORY);
    let (code, output) = cli(&["analyze", &project.display().to_string(), "--format", "dot"]);

    assert_eq!(code, 0, "{output}");
    assert!(output.contains("// entry: main.start"), "{output}");
    assert!(output.contains("digraph story {"), "{output}");
    assert!(
        output.contains("\"main.start\" -> \"main.ending\";"),
        "{output}"
    );
    assert!(
        output.contains("\"main.orphan\" [style=dashed];"),
        "an unreachable label should be drawn, and drawn as unreachable: {output}"
    );
}

/// A format nobody implements is a usage error rather than an empty report: a build script asking for
/// `yaml` and getting nothing would record "no findings" for every run.
#[test]
fn an_unknown_format_is_refused() {
    let project = temp_project("analyze-format", STORY);
    let (code, output) = cli(&[
        "analyze",
        &project.display().to_string(),
        "--format",
        "yaml",
    ]);

    assert_eq!(code, 2, "{output}");
    assert!(output.contains("expected text, json, or dot"), "{output}");
}
