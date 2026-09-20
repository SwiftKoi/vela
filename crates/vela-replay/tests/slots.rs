//! A directory of slots, listed without loading any of them (`RUNTIME.md §5`).
//!
//! This is the list a save screen draws before it draws a world, so what matters here is what it says
//! about the files that are *wrong*: a save from a newer build, a half-written file, a file that is not
//! a save at all. Each is a slot that is still a slot — the player has to be able to see it, and to
//! delete it — which is the one property the whole module exists for.

use std::path::{Path, PathBuf};

use vela_replay::{SAVE_VERSION, Save, checksum, path_of, slots};
use vela_vm::{Snapshot, VmState};
use vela_world::{Value, World};

/// A save with one value in its world, filed under `slot`.
fn slot_save(slot: &str) -> Save {
    let mut world = World::new();
    world.set("trust", Value::Int(3));
    let snapshot = Snapshot {
        world,
        vm: VmState::default(),
        current: None,
    };
    Save::new(snapshot, [0u8; 32], slot)
}

/// A directory of its own per test, removed first.
fn directory(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vela-slots-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create the directory");
    dir
}

/// Rewrites a save's version and re-checksums it, standing in for another build's file.
fn with_version(bytes: &[u8], version: u16) -> Vec<u8> {
    let mut out = bytes.to_vec();
    out[4..6].copy_from_slice(&version.to_le_bytes());
    let end = out.len() - 8;
    let sum = checksum(&out[..end]);
    out[end..].copy_from_slice(&sum.to_le_bytes());
    out
}

/// Every slot is listed once, by name, and a real one is loadable.
#[test]
fn a_directory_lists_its_slots_by_name() {
    let dir = directory("list");
    for name in ["quick", "auto", "1-2", "1-1"] {
        slot_save(name)
            .write_atomic(&path_of(&dir, name))
            .expect("write");
    }

    let found = slots(&dir).expect("list");
    let names: Vec<&str> = found.iter().map(|slot| slot.name.as_str()).collect();
    assert_eq!(names, vec!["1-1", "1-2", "auto", "quick"], "by name");
    for slot in &found {
        assert!(slot.loadable, "{slot:?}");
        assert_eq!(slot.version, Some(SAVE_VERSION));
        assert_eq!(slot.time, 0, "the host stamps this, not the store");
    }
    std::fs::remove_dir_all(&dir).expect("clean up");
}

/// A directory that does not exist is no slots, which is the first state a save screen draws.
#[test]
fn a_directory_that_is_not_there_is_no_slots() {
    let missing = std::env::temp_dir().join(format!("vela-slots-missing-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&missing);
    assert!(slots(&missing).expect("list").is_empty());
}

/// A save from a newer build keeps its place and is reported unloadable.
#[test]
fn a_save_from_the_future_is_listed_and_unloadable() {
    let dir = directory("future");
    let bytes = with_version(
        &slot_save("quick").to_bytes().expect("encode"),
        SAVE_VERSION + 1,
    );
    std::fs::write(path_of(&dir, "quick"), bytes).expect("write");

    let found = slots(&dir).expect("list");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "quick");
    assert_eq!(
        found[0].version,
        Some(SAVE_VERSION + 1),
        "the version is still readable"
    );
    assert!(
        !found[0].loadable,
        "a save this build cannot read is not loadable"
    );
    std::fs::remove_dir_all(&dir).expect("clean up");
}

/// A half-written save — and a file that is not a save at all — are slots too.
#[test]
fn a_broken_file_is_a_slot_that_cannot_be_loaded() {
    let dir = directory("broken");
    let bytes = slot_save("quick").to_bytes().expect("encode");
    std::fs::write(path_of(&dir, "quick"), &bytes[..bytes.len() - 40]).expect("truncate");
    std::fs::write(path_of(&dir, "junk"), b"this is not a save").expect("junk");

    let found = slots(&dir).expect("list");
    let names: Vec<&str> = found.iter().map(|slot| slot.name.as_str()).collect();
    assert_eq!(names, vec!["junk", "quick"]);
    for slot in &found {
        assert!(!slot.loadable, "{slot:?}");
    }
    assert_eq!(
        found[0].version, None,
        "a non-save says nothing about a version"
    );
    std::fs::remove_dir_all(&dir).expect("clean up");
}

/// A file that is not a slot is not listed: the extension is what makes a save a save here.
#[test]
fn only_files_with_the_slot_extension_are_slots() {
    let dir = directory("extension");
    std::fs::write(dir.join("notes.txt"), b"todo").expect("write");
    std::fs::write(dir.join("quick.velasave.tmp"), b"half a write").expect("write");

    assert!(slots(&dir).expect("list").is_empty());
    std::fs::remove_dir_all(&dir).expect("clean up");
}

/// An older save's metadata is readable while its world is not this build's business.
///
/// The reason `header_of` exists as well as `read`: a list may not need a schema or a migrator, and the
/// corpus's oldest save is exactly the file that proves it.
#[test]
fn an_older_saves_metadata_reads_without_a_migrator() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/saves/v1_fixture.velasave");
    let bytes = std::fs::read(&path).expect("the v1 fixture is in the corpus");

    let header = Save::header_of(&bytes).expect("the metadata reads");
    assert_eq!(header.save_version, 1);
    assert!(
        Save::from_bytes(&bytes).is_err(),
        "the world is not v1's to read"
    );

    let dir = directory("older");
    std::fs::copy(&path, path_of(&dir, "v1_fixture")).expect("copy the fixture");
    let found = slots(&dir).expect("list");
    assert_eq!(found.len(), 1);
    assert!(
        found[0].loadable,
        "an older save is loadable: `Save::load` migrates it"
    );
    assert_eq!(found[0].version, Some(1));
    std::fs::remove_dir_all(&dir).expect("clean up");
}
