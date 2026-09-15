//! Tests for the CLI's own behaviour: dispatch, scaffolding, checking, and running.

use crate::VERSION;
use crate::command::Command;
use crate::commands::NewProject;

use super::support::{cli, temp_project};

#[test]
fn version_is_printed_and_exits_zero() {
    let (code, out) = cli(&["--version"]);
    assert_eq!(code, 0);
    assert_eq!(out, format!("vela {VERSION}\n"));
}

#[test]
fn short_version_flag_works_too() {
    let (code, out) = cli(&["-V"]);
    assert_eq!(code, 0);
    assert!(out.starts_with("vela "));
}

#[test]
fn help_lists_registered_commands_and_comes_from_the_registry() {
    let (code, out) = cli(&["--help"]);
    assert_eq!(code, 0);
    // The listing is generated from the registry, so `new` must appear because it is
    // registered, not because the help text was edited.
    assert!(out.contains("new"), "{out}");
    assert!(out.contains("create a new project"), "{out}");
    assert!(out.contains("--version"), "{out}");
}

#[test]
fn bare_invocation_prints_help_without_erroring() {
    let (code, out) = cli(&[]);
    assert_eq!(code, 0);
    assert!(out.contains("usage: vela"));
}

#[test]
fn unknown_command_is_a_usage_error() {
    let (code, out) = cli(&["frobnicate"]);
    assert_eq!(code, 2);
    assert!(out.contains("unknown command `frobnicate`"), "{out}");
}

#[test]
fn unknown_option_is_a_usage_error() {
    let (code, out) = cli(&["--wat"]);
    assert_eq!(code, 2);
    assert!(out.contains("unknown option `--wat`"), "{out}");
}

#[test]
fn new_scaffolds_a_project_and_refuses_to_overwrite_it() {
    let base = std::env::temp_dir().join(format!("vela-cli-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).expect("create temp base");

    let command = NewProject::new(&base);
    let mut out: Vec<u8> = Vec::new();
    command
        .run(&["demo".to_string()], &mut out)
        .expect("first run succeeds");

    let dir = base.join("demo");
    assert!(dir.join("vela.toml").is_file());
    assert!(dir.join("src").join("main.vela").is_file());
    assert!(dir.join(".gitignore").is_file());

    let manifest = std::fs::read_to_string(dir.join("vela.toml")).expect("read manifest");
    assert!(manifest.contains("schema = 1"), "{manifest}");
    assert!(manifest.contains("name = \"demo\""), "{manifest}");
    assert!(manifest.contains("entry = \"main.start\""), "{manifest}");

    // The scaffolded story must be one line of dialogue (VISION.md §1).
    let story = std::fs::read_to_string(dir.join("src").join("main.vela")).expect("read story");
    assert!(story.contains("\"Hello, world.\""), "{story}");

    // Running it again must refuse rather than clobber.
    let error = command
        .run(&["demo".to_string()], &mut Vec::new())
        .expect_err("second run refuses");
    assert_eq!(error.code, 2);

    std::fs::remove_dir_all(&base).expect("clean up temp base");
}

#[test]
fn new_rejects_names_that_are_not_identifiers() {
    let base = std::env::temp_dir().join(format!("vela-cli-name-test-{}", std::process::id()));
    let command = NewProject::new(&base);

    for bad in ["../escape", "with space", "", "-flag"] {
        let error = command
            .run(&[bad.to_string()], &mut Vec::new())
            .expect_err("should be rejected");
        assert_eq!(error.code, 2, "`{bad}` should be a usage error");
    }
}

#[test]
fn json_output_has_the_documented_shape() {
    let project = temp_project("json", "label start:\n    jump nope\n    return\n");
    let (code, out) = cli(&["check", "--format", "json", &project.to_string_lossy()]);
    assert_eq!(code, 1);

    let value: serde_json::Value = serde_json::from_str(out.trim()).expect("valid json");
    let first = &value[0];
    assert_eq!(first["code"], "E5003");
    assert_eq!(first["severity"], "error");
    assert!(first["spans"].is_array(), "spans");
    assert!(first["notes"].is_array(), "notes");
    assert!(first["suggestions"].is_array(), "suggestions");

    // 1-based, like every consumer of this expects.
    assert_eq!(first["spans"][0]["startLine"], 2);
}

#[test]
fn sarif_output_has_the_required_structure() {
    let project = temp_project("sarif", "label start:\n    jump nope\n    return\n");
    let (code, out) = cli(&["check", "--format", "sarif", &project.to_string_lossy()]);
    assert_eq!(code, 1);

    let value: serde_json::Value = serde_json::from_str(out.trim()).expect("valid json");
    assert_eq!(value["version"], "2.1.0");
    assert!(
        value["$schema"]
            .as_str()
            .is_some_and(|schema| schema.contains("sarif")),
        "a viewer needs the schema"
    );

    let run = &value["runs"][0];
    assert_eq!(run["tool"]["driver"]["name"], "vela");
    let rules = run["tool"]["driver"]["rules"].as_array().expect("rules");
    let results = run["results"].as_array().expect("results");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["ruleId"], "E5003");
    assert_eq!(results[0]["level"], "error");

    // Every result has to name a rule the driver declares, or a viewer shows a bare id
    // where the explanation should be.
    for result in results {
        let id = result["ruleId"].as_str().expect("a rule id");
        assert!(
            rules.iter().any(|rule| rule["id"] == id),
            "{id} has no rule"
        );
    }
}

#[test]
fn a_machine_format_carries_no_human_summary() {
    let project = temp_project("machine", "label start:\n    jump nope\n    return\n");
    let path = project.to_string_lossy().to_string();

    for format in ["json", "sarif"] {
        let (_, out) = cli(&["check", "--format", format, &path]);
        assert!(
            serde_json::from_str::<serde_json::Value>(out.trim()).is_ok(),
            "{format} output is not parseable: {out}"
        );
    }

    // The tally still reaches a person, on the stream a person reads.
    let (_, human) = cli(&["check", &path]);
    assert!(human.contains("error(s)"), "{human}");
}

#[test]
fn an_unknown_format_is_a_usage_error() {
    let (code, out) = cli(&["check", "--format", "yaml", "."]);
    assert_eq!(code, 2);
    assert!(out.contains("unknown format"), "{out}");
}

#[test]
fn emit_mir_prints_the_linked_program() {
    let project = temp_project("mir", "label start:\n    \"Hi.\"\n    return\n");
    let (code, out) = cli(&["check", "--emit", "mir", &project.to_string_lossy()]);

    assert_eq!(code, 0, "{out}");
    // The *program*, not a module: what is printed is what a run executes, and its labels are the
    // qualified names linking gives them (`LANGUAGE.md §6`).
    assert!(out.contains("module program"), "{out}");
    assert!(out.contains("label main.start()"), "{out}");
    assert!(out.contains("cmd say("), "{out}");
    assert!(out.contains("yield say"), "{out}");

    // A summary appended to the document would make it undiffable.
    assert!(!out.contains("no problems"), "{out}");
}

#[test]
fn an_unknown_emit_is_a_usage_error() {
    let (code, out) = cli(&["check", "--emit", "bytecode", "."]);
    assert_eq!(code, 2);
    assert!(out.contains("unknown emit"), "{out}");
}

#[test]
fn emit_mir_still_reports_a_lowering_problem() {
    // A `const` the compiler cannot evaluate is reported by lowering, and its diagnostic
    // has to reach the terminal rather than only the MIR.
    let project = temp_project(
        "mir-diag",
        "fn compute() -> int:\n    return 3\n\nconst LIMIT: int = compute()\n",
    );
    let (code, out) = cli(&["check", "--emit", "mir", &project.to_string_lossy()]);

    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E2004"), "{out}");
}

#[test]
fn emit_disasm_prints_a_verified_listing() {
    let project = temp_project("disasm", "label start:\n    \"Hi.\"\n    return\n");
    let (code, out) = cli(&["check", "--emit", "disasm", &project.to_string_lossy()]);

    assert_eq!(code, 0, "{out}");
    assert!(out.contains("label 0 `main.start`"), "{out}");
    assert!(out.contains("const.s"), "{out}");
    assert!(out.contains("cmd"), "{out}");
    assert!(out.contains("return"), "{out}");

    // Offsets are what a jump target refers to, so a listing without them is not a tool.
    assert!(out.contains("0000"), "{out}");
    // A verifier failure would have been rendered here instead.
    assert!(!out.contains("E6"), "{out}");
}

#[test]
fn run_headless_prints_the_command_stream() {
    // The milestone's demo: a story runs to a halt with no window, printing what it says.
    let project = temp_project("run", "label start:\n    \"Hi.\"\n    return\n");
    let (code, out) = cli(&[
        "run",
        "--headless",
        "--start",
        "main.start",
        &project.to_string_lossy(),
    ]);

    assert_eq!(code, 0, "{out}");
    assert_eq!(out.trim(), "say \"Hi.\"");
}

#[test]
fn run_still_asks_for_a_project() {
    // Without `--headless` this opens a window, so the thing a test can still check is that
    // it resolves the project *first* and says what is missing. Opening a window is not
    // something a unit test should do, and a test that did would be a test that fails on a
    // machine with no display.
    let (code, out) = cli(&["run", "."]);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("vela.toml"), "{out}");
}

#[test]
fn headless_is_still_accepted() {
    let project = temp_project("run-headless", "label start:\n    \"Hi.\"\n    return\n");
    let (code, out) = cli(&["run", "--headless", &project.to_string_lossy()]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(out.trim(), "say \"Hi.\"");
}

#[test]
fn run_refuses_a_project_with_errors() {
    let project = temp_project("run-bad", "label start:\n    jump nowhere\n    return\n");
    let (code, out) = cli(&["run", "--headless", &project.to_string_lossy()]);

    assert_eq!(code, 1, "{out}");
    assert!(out.contains("errors"), "{out}");
}

#[test]
fn a_window_is_titled_by_the_project_not_by_its_entry_point() {
    use crate::commands::run::title_of;
    use crate::manifest::{Manifest, Project};

    let named = Manifest {
        schema: 1,
        project: Project {
            name: Some("standard".to_string()),
            entry: "main.start".to_string(),
        },
    };
    assert_eq!(
        title_of(Some(&named), "main.start"),
        "standard",
        "a title bar reading `main.start` is a label path, not the name of a game"
    );

    // A project that does not name itself still opens, titled by where it starts.
    let unnamed = Manifest {
        schema: 1,
        project: Project {
            name: None,
            entry: "main.start".to_string(),
        },
    };
    assert_eq!(title_of(Some(&unnamed), "main.start"), "main.start");
    assert_eq!(title_of(None, "main.start"), "main.start");
}
