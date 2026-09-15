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

/// A story with screens *and* a line that must not end up in the bundle.
const SCREENED: &str = "\
screen dialogue(name: str?, line: str):
    box at bottom, stretch_x:
        text line

screen pause:
    box stretch_x, stretch_y:
        text \"Paused\"

label start:
    \"STORY-SENTINEL.\"
    return
";

#[test]
fn a_bundle_carries_the_project_screens_as_a_compiled_pack() {
    let project = temp_project("bundle-screens", SCREENED);
    let (code, out) = cli(&["build", &project.to_string_lossy()]);
    assert_eq!(code, 0, "{out}");

    // A compiled pack, not the declaration text: a binary container, no source syntax, and no
    // story in it.
    let pack = std::fs::read(project.join("dist/screens/main.velspk"))
        .expect("the bundle carries a compiled screen pack");
    assert!(pack.starts_with(b"VELS"), "the pack has no magic number");
    assert!(
        !pack.windows(6).any(|window| window == b"screen"),
        "the pack is source text"
    );
    assert!(
        !pack.windows(14).any(|window| window == b"STORY-SENTINEL"),
        "the story is in the pack"
    );

    // And the runtime loads it without a parser, which is what makes Escape open `pause` and a
    // `dialogue` screen draw — a built game has to be the game that was tested.
    let screens = crate::commands::ui::Screens::load_bundle(&project.join("dist"))
        .expect("the packed screens did not load");
    assert!(screens.has("pause"), "the packed screens did not load");
    assert!(screens.has("dialogue"), "the packed screens did not load");
}

#[test]
fn a_screen_pack_from_a_newer_build_is_refused() {
    let project = temp_project("bundle-pack-version", SCREENED);
    let (code, out) = cli(&["build", &project.to_string_lossy()]);
    assert_eq!(code, 0, "{out}");

    // The version sits right after the four magic bytes, so a pack from a newer build is one edit
    // away — which is what someone with two builds on disk will have.
    let path = project.join("dist/screens/main.velspk");
    let mut pack = std::fs::read(&path).expect("a pack");
    assert!(pack.starts_with(b"VELS"), "the pack has no magic number");
    pack[4..6].copy_from_slice(&99u16.to_le_bytes());
    std::fs::write(&path, pack).expect("write the pack");

    let error = crate::commands::ui::Screens::load_bundle(&project.join("dist"))
        .err()
        .expect("a pack this build does not know is refused");
    assert!(error.to_string().contains("99"), "{error}");
}

#[test]
fn a_project_without_screens_builds_a_bundle_without_them() {
    let project = temp_project(
        "bundle-no-screens",
        "label start:\n    \"Hi.\"\n    return\n",
    );
    let (code, out) = cli(&["build", &project.to_string_lossy()]);
    assert_eq!(code, 0, "{out}");

    assert!(
        !project.join("dist/screens").exists(),
        "a project with no screens wrote a screens directory"
    );
    let screens = crate::commands::ui::Screens::load_bundle(&project.join("dist"))
        .expect("no screens is not an error");
    assert!(!screens.has("pause"));
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
