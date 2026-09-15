//! Tests for `vela build` and `vela patch`: the commands that produce artifacts.
//!
//! Separated from `cli_tests.rs` by responsibility rather than by line count — one file is the
//! commands a person types to look at a project, and this is the ones that write a bundle.

use super::support::{cli, temp_project};

/// Writes an asset into a project's `assets/` directory, creating it.
fn write_asset(project: &std::path::Path, name: &str, contents: &str) {
    let path = project.join("assets").join(name);
    std::fs::create_dir_all(path.parent().expect("has a parent")).expect("create assets");
    std::fs::write(&path, contents).expect("write the asset");
}

#[test]
fn build_writes_a_bundle_with_scripts_and_assets() {
    let project = temp_project("build", "label start:\n    \"Hi.\"\n    return\n");
    write_asset(&project, "art/hero.json", "{ \"hp\": 3 }");

    let (code, out) = cli(&["build", &project.to_string_lossy()]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("built 1 script(s), 1 asset(s), 1 artifact(s)"),
        "{out}"
    );

    let manifest = std::fs::read_to_string(project.join("dist/manifest.json")).expect("a manifest");
    assert!(manifest.contains("\"id\": \"art.hero\""), "{manifest}");
    // The artifact is written where the manifest says it is, which is the property that makes
    // the manifest usable rather than decorative.
    let artifact = project.join("dist/assets/art/hero.json");
    assert_eq!(
        std::fs::read_to_string(&artifact).expect("the artifact"),
        r#"{"hp":3}"#,
        "the artifact was not written, or was not normalized"
    );

    // And the script, as a container the runtime can load rather than a file named `.velac`.
    let script = std::fs::read(project.join("dist/scripts/main.velac")).expect("a compiled module");
    let module = vela_bytecode::decode(&script).expect("the bundle does not decode");
    assert!(
        module
            .labels
            .iter()
            .any(|label| module.strings.get(label.name) == Some("start")),
        "the compiled module has no `start` label"
    );
}

#[test]
fn build_refuses_a_story_that_does_not_check() {
    let project = temp_project(
        "build-broken",
        "label start:\n    jump nowhere\n    return\n",
    );

    let (code, out) = cli(&["build", &project.to_string_lossy()]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("refusing to build"), "{out}");
    assert!(
        !project.join("dist").exists(),
        "a refused build wrote a bundle anyway"
    );
}

#[test]
fn verify_reproducible_passes_and_says_so() {
    let project = temp_project(
        "build-reproducible",
        "label start:\n    \"Hi.\"\n    return\n",
    );
    write_asset(&project, "art/hero.json", "{ \"hp\": 3 }");

    let (code, out) = cli(&["build", "--verify-reproducible", &project.to_string_lossy()]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("byte-identical"), "{out}");
}

#[test]
fn build_compiles_every_module_under_src() {
    let project = temp_project("build-modules", "label start:\n    \"Hi.\"\n    return\n");
    let nested = project.join("src/chapters");
    std::fs::create_dir_all(&nested).expect("create the module directory");
    std::fs::write(
        nested.join("forest.vela"),
        "label clearing:\n    \"Trees.\"\n    return\n",
    )
    .expect("write the module");

    let (code, out) = cli(&["build", &project.to_string_lossy()]);
    assert_eq!(code, 0, "{out}");
    assert!(
        project.join("dist/scripts/chapters/forest.velac").is_file(),
        "the bundle does not mirror the source tree:\n{out}"
    );
}

#[test]
fn building_twice_produces_the_same_manifest() {
    let project = temp_project("build-twice", "label start:\n    \"Hi.\"\n    return\n");
    write_asset(&project, "theme.toml", "size = 3\n");
    write_asset(&project, "art/hero.json", "{ \"b\": 1, \"a\": 2 }");

    let (code, _) = cli(&["build", &project.to_string_lossy()]);
    assert_eq!(code, 0);
    let first = std::fs::read_to_string(project.join("dist/manifest.json")).expect("a manifest");

    let (code, _) = cli(&["build", &project.to_string_lossy()]);
    assert_eq!(code, 0);
    let second = std::fs::read_to_string(project.join("dist/manifest.json")).expect("a manifest");

    assert_eq!(first, second, "two builds of one tree differ");
}

#[test]
fn build_without_assets_is_an_empty_manifest() {
    let project = temp_project("build-empty", "label start:\n    \"Hi.\"\n    return\n");

    let (code, out) = cli(&["build", &project.to_string_lossy()]);
    assert_eq!(code, 0, "{out}");

    let manifest = std::fs::read_to_string(project.join("dist/manifest.json")).expect("a manifest");
    assert!(manifest.contains("\"assets\": []"), "{manifest}");
}

#[test]
fn build_moves_the_destination_with_out() {
    let project = temp_project("build-out", "label start:\n    \"Hi.\"\n    return\n");
    write_asset(&project, "theme.toml", "size = 3\n");
    let elsewhere = project.join("elsewhere");

    let (code, out) = cli(&[
        "build",
        "--out",
        &elsewhere.to_string_lossy(),
        &project.to_string_lossy(),
    ]);
    assert_eq!(code, 0, "{out}");
    assert!(elsewhere.join("manifest.json").is_file(), "{out}");
    assert!(
        !project.join("dist").exists(),
        "the default was written too"
    );
}

#[test]
fn build_refuses_an_asset_nothing_imports() {
    let project = temp_project("build-unclaimed", "label start:\n    \"Hi.\"\n    return\n");
    write_asset(&project, "mystery.bin", "\u{0}\u{1}");

    let (code, out) = cli(&["build", &project.to_string_lossy()]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("mystery.bin"), "{out}");
    assert!(
        !project.join("dist").exists(),
        "a failed build wrote output"
    );
}

#[test]
fn build_refuses_a_directory_that_is_not_a_project() {
    let directory =
        std::env::temp_dir().join(format!("vela-cli-not-a-project-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("create the directory");

    let (code, out) = cli(&["build", &directory.to_string_lossy()]);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("vela.toml"), "{out}");
}

#[test]
fn check_reports_a_path_that_is_not_an_asset() {
    let project = temp_project(
        "check-asset-missing",
        "image hero = @\"art/hero.json\"\n\nlabel start:\n    play music @\"audio/theme.ogg\"\n    \"Hi.\"\n    return\n",
    );
    write_asset(&project, "art/hero.json", "{ \"hp\": 3 }");

    let (code, out) = cli(&["check", &project.to_string_lossy()]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E7001"), "{out}");
    assert!(out.contains("audio/theme.ogg"), "{out}");
    assert!(
        !out.contains("art/hero.json"),
        "the asset that exists was reported too:\n{out}"
    );
}

#[test]
fn check_is_quiet_when_every_path_is_an_asset() {
    let project = temp_project(
        "check-asset-present",
        "image hero = @\"art/hero.json\"\n\nlabel start:\n    \"Hi.\"\n    return\n",
    );
    write_asset(&project, "art/hero.json", "{ \"hp\": 3 }");

    let (code, out) = cli(&["check", &project.to_string_lossy()]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("no problems"), "{out}");
}

#[test]
fn check_without_an_assets_directory_still_reports_a_missing_path() {
    // A project with no `assets/` has an empty manifest, which is not the same as having no
    // manifest: a literal in it names nothing, and the author needs to be told.
    let project = temp_project(
        "check-no-assets",
        "image hero = @\"art/hero.json\"\n\nlabel start:\n    \"Hi.\"\n    return\n",
    );

    let (code, out) = cli(&["check", &project.to_string_lossy()]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E7001"), "{out}");
}

/// Every file under a directory, by path, with its bytes — what "the same build" means.
fn tree(root: &std::path::Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    let mut found = std::collections::BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).expect("read the tree").flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let name = path
                    .strip_prefix(root)
                    .expect("inside the tree")
                    .to_string_lossy()
                    .replace('\\', "/");
                found.insert(name, std::fs::read(&path).expect("read a file"));
            }
        }
    }
    found
}

/// Copies a directory tree.
fn copy_tree(from: &std::path::Path, to: &std::path::Path) {
    for (name, bytes) in tree(from) {
        let path = to.join(name);
        std::fs::create_dir_all(path.parent().expect("has a parent")).expect("create");
        std::fs::write(&path, bytes).expect("write");
    }
}

#[test]
fn a_patch_brings_a_previous_build_up_to_date() {
    let project = temp_project("patch", "label start:\n    \"One.\"\n    return\n");
    write_asset(&project, "art/hero.json", "{ \"hp\": 3 }");

    let first = project.join("release-1");
    let second = project.join("release-2");
    let patch = project.join("patch-1");

    let (code, out) = cli(&[
        "build",
        &project.to_string_lossy(),
        "--out",
        &first.to_string_lossy(),
    ]);
    assert_eq!(code, 0, "{out}");

    // The text-only change `VISION.md §5`'s acceptance bar is about.
    std::fs::write(
        project.join("src/main.vela"),
        "label start:\n    \"Two.\"\n    return\n",
    )
    .expect("edit the story");

    let (code, out) = cli(&[
        "build",
        &project.to_string_lossy(),
        "--out",
        &second.to_string_lossy(),
        "--patch-from",
        &first.to_string_lossy(),
        "--patch-out",
        &patch.to_string_lossy(),
    ]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("patch:"), "no patch was reported:\n{out}");

    // A player's copy of release 1, brought forward by the patch alone.
    let player = project.join("player");
    copy_tree(&first, &player);
    let (code, out) = cli(&[
        "patch",
        "apply",
        &patch.to_string_lossy(),
        &player.to_string_lossy(),
    ]);
    assert_eq!(code, 0, "{out}");

    assert_eq!(
        tree(&player),
        tree(&second),
        "the patched build is not release 2"
    );
}

#[test]
fn a_patch_for_another_build_is_refused() {
    let project = temp_project("patch-wrong", "label start:\n    \"One.\"\n    return\n");
    let first = project.join("release-1");
    let second = project.join("release-2");
    let patch = project.join("patch-1");

    let (code, _) = cli(&[
        "build",
        &project.to_string_lossy(),
        "--out",
        &first.to_string_lossy(),
    ]);
    assert_eq!(code, 0);
    std::fs::write(
        project.join("src/main.vela"),
        "label start:\n    \"Two.\"\n    return\n",
    )
    .expect("edit the story");
    let (code, _) = cli(&[
        "build",
        &project.to_string_lossy(),
        "--out",
        &second.to_string_lossy(),
        "--patch-from",
        &first.to_string_lossy(),
        "--patch-out",
        &patch.to_string_lossy(),
    ]);
    assert_eq!(code, 0);

    // Somebody else's build: a copy of release 2 is not what the patch is for.
    let stranger = project.join("stranger");
    copy_tree(&second, &stranger);
    let before = tree(&stranger);
    let (code, out) = cli(&[
        "patch",
        "apply",
        &patch.to_string_lossy(),
        &stranger.to_string_lossy(),
    ]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("different build"), "{out}");
    assert_eq!(before, tree(&stranger), "the refused apply wrote anyway");
}

#[test]
fn patch_from_without_patch_out_is_a_usage_error() {
    let project = temp_project("patch-usage", "label start:\n    \"One.\"\n    return\n");

    let (code, out) = cli(&[
        "build",
        &project.to_string_lossy(),
        "--patch-from",
        &project.to_string_lossy(),
    ]);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("--patch-out"), "{out}");
}

#[test]
fn patch_without_a_subcommand_is_a_usage_error() {
    let (code, out) = cli(&["patch"]);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("apply"), "{out}");
}

#[test]
fn an_option_this_command_does_not_have_is_refused() {
    // Silently ignoring one is how `vela build --targt web` looks like it built for web. The
    // first evidence otherwise would be a player, so it is a usage error instead.
    let project = temp_project("build-target", "label start:\n    \"Hi.\"\n    return\n");

    let (code, out) = cli(&["build", &project.to_string_lossy(), "--wat"]);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("--wat"), "{out}");
    assert!(
        !project.join("dist").exists(),
        "a refused build wrote output"
    );
}

#[test]
fn an_unknown_target_is_a_usage_error() {
    let project = temp_project(
        "build-bad-target",
        "label start:\n    \"Hi.\"\n    return\n",
    );

    let (code, out) = cli(&["build", &project.to_string_lossy(), "--target", "amiga"]);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("amiga"), "{out}");
    assert!(
        out.contains("win"),
        "the message should list what there is:\n{out}"
    );
    assert!(
        !project.join("dist").exists(),
        "a refused build wrote output"
    );
}

#[test]
fn a_target_builds_one_bundle_with_a_descriptor_and_a_launcher() {
    let project = temp_project("build-targets", "label start:\n    \"Hi.\"\n    return\n");
    let dist = project.join("dist");

    let (code, out) = cli(&["build", &project.to_string_lossy(), "--target", "linux,web"]);
    assert_eq!(code, 0, "{out}");

    // Each target is a self-contained bundle: the story is target-independent, so the same
    // scripts and assets appear in both, and only the descriptor and launcher differ.
    for (target, launcher) in [("linux", "launch.sh"), ("web", "index.html")] {
        let dir = dist.join(target);
        assert!(
            dir.join("manifest.json").is_file(),
            "{target} has no manifest"
        );
        assert!(dir.join("scripts/main.velac").is_file(), "{target}");
        assert!(
            dir.join("target.json").is_file(),
            "{target} has no descriptor"
        );
        assert!(dir.join(launcher).is_file(), "{target} has no launcher");
    }

    // A target is not cosmetic: the descriptors say different things, and the launchers are
    // written for different systems.
    let linux = std::fs::read_to_string(dist.join("linux/target.json")).expect("linux descriptor");
    let web = std::fs::read_to_string(dist.join("web/target.json")).expect("web descriptor");
    assert!(linux.contains("\"backend\": \"vulkan\""), "{linux}");
    assert!(web.contains("\"backend\": \"webgpu-webgl2\""), "{web}");
    assert_ne!(linux, web, "two targets wrote the same descriptor");

    // The web launcher is a page; the desktop one is a shell script that runs the engine on
    // this very bundle.
    let web_page = std::fs::read_to_string(dist.join("web/index.html")).expect("web page");
    assert!(web_page.contains("Player"), "{web_page}");
    let shell = std::fs::read_to_string(dist.join("linux/launch.sh")).expect("shell launcher");
    assert!(shell.contains("run"), "{shell}");

    // `--target` builds the targets, not a plain bundle at the root: the root is what a
    // single-target build uses, and leaving one there would be a second thing to explain.
    assert!(
        !dist.join("manifest.json").exists(),
        "a root bundle was written too"
    );
}
