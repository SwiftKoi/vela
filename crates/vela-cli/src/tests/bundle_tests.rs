//! Tests for running a *built* bundle.
//!
//! This is the check that makes M9's launcher a fact rather than a claim: a bundle plays the
//! same story as the source it was built from, with **no compiler in the path**. The strongest
//! way to observe that is to delete the source tree and run the bundle anyway — if anything
//! recompiled, there would be nothing to compile.

use super::support::{cli, temp_project};

/// A story with a branch, so the command stream is long enough to be evidence of agreement.
const STORY: &str = "\
default trust: int = 0

label start:
    \"You arrive.\"
    menu:
        \"Stay\":
            trust = 1
            jump stay
        \"Leave\":
            jump leave
    return

label stay:
    \"You stay.\"
    return

label leave:
    \"You leave.\"
    return
";

/// The command stream a project prints when run headless.
fn from_source(project: &std::path::Path) -> String {
    let (code, out) = cli(&["run", "--headless", &project.to_string_lossy()]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("say \"You arrive.\""), "{out}");
    out
}

#[test]
fn a_built_bundle_runs_without_the_compiler() {
    let project = temp_project("bundle-run", STORY);
    let source = from_source(&project);

    let (code, out) = cli(&["build", &project.to_string_lossy()]);
    assert_eq!(code, 0, "{out}");

    // The whole point: the source is gone, so nothing here can recompile it.
    std::fs::remove_dir_all(project.join("src")).expect("remove the source tree");
    let dist = project.join("dist");
    let (code, out) = cli(&["run", "--headless", &dist.to_string_lossy()]);

    assert_eq!(code, 0, "{out}");
    assert_eq!(
        source, out,
        "the bundle and the source played different stories"
    );
}

#[test]
fn a_bundle_built_for_a_target_plays_the_same_story() {
    let project = temp_project("bundle-target-run", STORY);
    let source = from_source(&project);

    let (code, out) = cli(&["build", &project.to_string_lossy(), "--target", "linux"]);
    assert_eq!(code, 0, "{out}");

    std::fs::remove_dir_all(project.join("src")).expect("remove the source tree");
    let dist = project.join("dist").join("linux");
    let (code, out) = cli(&["run", "--headless", &dist.to_string_lossy()]);

    assert_eq!(code, 0, "{out}");
    assert_eq!(
        source, out,
        "a target bundle played a different story from the source"
    );
}

#[test]
fn a_bundle_that_says_nothing_about_where_to_start_is_reported() {
    let dir = std::env::temp_dir().join(format!("vela-cli-empty-bundle-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("scripts")).expect("create the bundle");
    std::fs::write(
        dir.join("manifest.json"),
        "{\n  \"manifest_version\": 1,\n  \"assets\": []\n}\n",
    )
    .expect("write the manifest");

    let (code, out) = cli(&["run", "--headless", &dir.to_string_lossy()]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("does not say where the story starts"), "{out}");
}
