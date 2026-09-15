//! Loading a story from a built bundle (`RUNTIME.md §8`, `BUILD_AND_ASSETS.md §1`).
//!
//! The claim under test is narrow and load-bearing: a bundle goes straight to the machine, its
//! entry point comes from the manifest, and no `.vela` source is involved. The end-to-end check
//! that a *built project* plays the same story as its source is in
//! `crates/vela-cli/src/tests/bundle_tests.rs`, where a real build runs.

mod common;

use std::path::{Path, PathBuf};

use vela_bytecode::Module;
use vela_vm::{Host, Session, Step, TakeFirst};

/// A directory of its own for one test, so two tests cannot see each other's files.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vela-vm-load-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create the scratch directory");
    dir
}

/// Writes `module` into `dir` as a bundle whose manifest names `entry`.
fn write_bundle(dir: &Path, module: &Module, entry: &str) {
    std::fs::create_dir_all(dir.join("scripts")).expect("create scripts");
    std::fs::write(
        dir.join("scripts/main.velac"),
        vela_bytecode::encode(module),
    )
    .expect("write the module");
    let manifest = format!(
        "{{\n  \"manifest_version\": 1,\n  \"name\": \"fixture\",\n  \"entry\": \"{entry}\",\n  \
         \"assets\": []\n}}\n"
    );
    std::fs::write(dir.join("manifest.json"), manifest).expect("write the manifest");
}

/// Plays a loaded session to its halt, answering the way a headless run does.
fn play(mut session: Session) -> Vec<String> {
    let mut host = TakeFirst;
    let mut commands = Vec::new();
    let mut step = session.advance();

    loop {
        match step {
            Step::Yield(command) => {
                let answer = host.answer(&command);
                commands.push(command.to_string());
                step = session.answer(answer);
            }
            Step::Continue => step = session.advance(),
            Step::Halt => break,
            Step::Fault(fault) => panic!("the loaded story faulted: {fault}"),
        }
    }
    commands
}

#[test]
fn a_lone_module_starts_at_its_first_label() {
    let dir = scratch("lone");
    let module = common::compile("main", "label start:\n    \"Hi.\"\n    return\n");
    let path = dir.join("story.velac");
    std::fs::write(&path, vela_bytecode::encode(&module)).expect("write the module");

    let session = Session::load(&path).expect("a module with a label loads");
    assert_eq!(play(session), vec!["say \"Hi.\""]);
}

#[test]
fn a_bundle_starts_where_its_manifest_says() {
    let dir = scratch("bundle");
    // `later` is the *second* label, so a loader that guessed "the first label" would play the
    // wrong story and this test would catch it.
    let module = common::compile(
        "main",
        "label start:\n    \"Start.\"\n    return\n\nlabel later:\n    \"Later.\"\n    return\n",
    );
    write_bundle(&dir, &module, "main.later");

    let session = Session::load(&dir).expect("the bundle loads");
    assert_eq!(play(session), vec!["say \"Later.\""]);
}

#[test]
fn a_directory_that_is_not_a_bundle_is_reported() {
    let dir = scratch("not-a-bundle");
    let error = Session::load(&dir)
        .err()
        .expect("a bare directory is not a bundle");

    assert!(
        matches!(error, vela_vm::LoadError::NotABundle(_)),
        "expected a `not a bundle` error, got {error:?}"
    );
}

#[test]
fn a_bundle_whose_module_is_missing_is_reported() {
    let dir = scratch("no-module");
    std::fs::write(
        dir.join("manifest.json"),
        "{\n  \"manifest_version\": 1,\n  \"entry\": \"main.start\"\n}\n",
    )
    .expect("write the manifest");

    let error = Session::load(&dir)
        .err()
        .expect("there is no module to load");
    assert!(
        matches!(error, vela_vm::LoadError::Io { .. }),
        "expected a read failure naming the module, got {error:?}"
    );
}

#[test]
fn an_entry_label_that_is_not_in_the_module_is_refused() {
    let dir = scratch("bad-label");
    let module = common::compile("main", "label start:\n    \"Hi.\"\n    return\n");
    write_bundle(&dir, &module, "main.nowhere");

    let error = Session::load(&dir)
        .err()
        .expect("the label is not in the module");
    assert!(
        matches!(error, vela_vm::LoadError::Fault(_)),
        "expected the start label to be refused, got {error:?}"
    );
}
