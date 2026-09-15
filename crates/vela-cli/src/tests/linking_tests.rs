//! Linking: a story split across files, run from source and from a bundle.
//!
//! This is the end of the feature `vela_mir::link` exists for. Before it, a cross-module `jump`
//! checked clean, built clean, and then faulted at run time on a label index that was never in the
//! module — so a multi-file project could not be played at all, and `vela check` said it was fine.

use std::path::PathBuf;

use super::support::cli;

/// Writes a project whose sources are the given files, by their paths under `src/`.
fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let base = std::env::temp_dir().join(format!("vela-cli-link-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("src")).expect("create src");
    std::fs::write(
        base.join("vela.toml"),
        "schema = 1\n\n[project]\nname = \"linked\"\nversion = \"0.1.0\"\nentry = \"main.start\"\n",
    )
    .expect("write vela.toml");

    for (path, text) in files {
        let file = base.join("src").join(path);
        std::fs::create_dir_all(file.parent().expect("a parent")).expect("create the directory");
        std::fs::write(&file, text).expect("write the module");
    }
    base
}

/// A menu in one module whose first choice jumps into another.
const MAIN: &str = "\
use chapters.forest as forest

label start:
    \"A.\"
    menu:
        \"Go\":
            jump forest.clearing
        \"Stay\":
            jump inside

label inside:
    \"C.\"
    return
";

const FOREST: &str = "label clearing:\n    \"B.\"\n    return\n";

#[test]
fn a_story_split_across_files_plays_from_source_and_from_a_bundle() {
    let project = project(
        "split",
        &[("main.vela", MAIN), ("chapters/forest.vela", FOREST)],
    );

    let (code, from_source) = cli(&["run", "--headless", &project.to_string_lossy()]);
    assert_eq!(code, 0, "{from_source}");
    assert!(
        from_source.contains("say \"B.\""),
        "the run never crossed a module boundary:\n{from_source}"
    );

    let (code, out) = cli(&["build", &project.to_string_lossy()]);
    assert_eq!(code, 0, "{out}");
    assert!(
        project.join("dist/scripts/main.velac").is_file(),
        "the program is named for its entry module:\n{out}"
    );

    // Without the sources: the bundle is the whole program, linked at build time.
    std::fs::remove_dir_all(project.join("src")).expect("remove the sources");
    let dist = project.join("dist");
    let (code, from_bundle) = cli(&["run", "--headless", &dist.to_string_lossy()]);

    assert_eq!(code, 0, "{from_bundle}");
    assert_eq!(
        from_source, from_bundle,
        "the bundle played a different story"
    );
}

#[test]
fn a_target_bundle_holds_the_linked_program_too() {
    let project = project(
        "split-target",
        &[("main.vela", MAIN), ("chapters/forest.vela", FOREST)],
    );

    let (code, out) = cli(&["build", &project.to_string_lossy(), "--target", "linux"]);
    assert_eq!(code, 0, "{out}");

    std::fs::remove_dir_all(project.join("src")).expect("remove the sources");
    let dist = project.join("dist/linux");
    let (code, out) = cli(&["run", "--headless", &dist.to_string_lossy()]);

    assert_eq!(code, 0, "{out}");
    assert!(out.contains("say \"B.\""), "{out}");
}

/// World state is global, so one name cannot be two slots — and neither module could have seen the
/// other. The failure names both, because that is the only place an author can look.
#[test]
fn a_default_declared_in_two_modules_is_refused() {
    let project = project(
        "duplicate-default",
        &[
            (
                "main.vela",
                "default trust: int = 1\n\nlabel start:\n    return\n",
            ),
            (
                "chapters/forest.vela",
                "default trust: int = 2\n\nlabel clearing:\n    return\n",
            ),
        ],
    );

    let (code, out) = cli(&["build", &project.to_string_lossy()]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("`default trust`"), "{out}");
    assert!(out.contains("chapters.forest"), "{out}");
    assert!(out.contains("main"), "{out}");
    assert!(
        !project.join("dist").exists(),
        "a refused build wrote a bundle"
    );
}

/// An entry point names a label in the linked program, so a module that is not there is a usage
/// error rather than a fault at the first instruction.
#[test]
fn an_entry_point_that_names_no_label_is_a_usage_error() {
    let project = project("bad-entry", &[("main.vela", "label start:\n    return\n")]);

    let (code, out) = cli(&[
        "run",
        "--headless",
        &project.to_string_lossy(),
        "--start",
        "main.nowhere",
    ]);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("main.nowhere"), "{out}");
}
