//! Delta patches: what a patch carries, and what it refuses.

use std::fs;
use std::path::{Path, PathBuf};

use crate::patch::Patch;
use crate::{AssetError, identity_of};

/// A directory of its own per test, removed first so a rerun starts clean.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vela-patch-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create the scratch directory");
    dir
}

/// Writes a file under `dir`, creating its parents.
fn write(dir: &Path, name: &str, bytes: &[u8]) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create the parent");
    }
    fs::write(&path, bytes).expect("write the fixture");
}

/// The bytes of every file in a tree, by path — what "the same bundle" means.
fn contents(root: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    let mut found = std::collections::BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("read the tree").flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let name = path
                    .strip_prefix(root)
                    .expect("inside the tree")
                    .to_string_lossy()
                    .replace('\\', "/");
                found.insert(name, fs::read(&path).expect("read a file"));
            }
        }
    }
    found
}

/// A patch carries what changed and nothing else.
#[test]
fn a_patch_carries_only_what_changed() {
    let root = scratch("between");
    let before = root.join("before");
    let after = root.join("after");
    write(&before, "scripts/main.velac", b"old script");
    write(&before, "assets/big.bin", &vec![7u8; 4096]);
    write(&before, "assets/gone.bin", b"removed");
    write(&after, "scripts/main.velac", b"new script");
    write(&after, "assets/big.bin", &vec![7u8; 4096]);
    write(&after, "assets/added.bin", b"added");

    let patch = Patch::write_between(&before, &after, &root.join("patch")).expect("writes");

    let paths: Vec<&str> = patch
        .entries
        .iter()
        .map(|entry| entry.path.as_str())
        .collect();
    assert_eq!(paths, vec!["assets/added.bin", "scripts/main.velac"]);
    assert_eq!(patch.removed, vec!["assets/gone.bin"]);

    // The unchanged 4 KiB file is the point: it is in the bundle, and not in the patch.
    let size = patch.size(&root.join("patch")).expect("measures");
    assert!(size < 4096, "the patch carried a file that did not change");
}

/// Applying a patch to the build it was computed from produces the new build exactly.
#[test]
fn applying_a_patch_produces_the_new_bundle() {
    let root = scratch("apply");
    let before = root.join("before");
    let after = root.join("after");
    write(&before, "manifest.json", b"{\"v\":1}");
    write(&before, "scripts/main.velac", b"old");
    write(&before, "assets/keep.bin", b"keep");
    write(&after, "manifest.json", b"{\"v\":2}");
    write(&after, "scripts/main.velac", b"new");
    write(&after, "assets/keep.bin", b"keep");

    let patch_root = root.join("patch");
    let patch = Patch::write_between(&before, &after, &patch_root).expect("writes");

    // The player has `before`; the patch arrives; the result must be `after`.
    patch.apply(&patch_root, &before).expect("applies");
    assert_eq!(contents(&before), contents(&after));
}

/// A removal is part of the difference, not an omission.
#[test]
fn applying_removes_what_the_new_bundle_lost() {
    let root = scratch("removed");
    let before = root.join("before");
    let after = root.join("after");
    write(&before, "assets/old.bin", b"old");
    write(&before, "assets/keep.bin", b"keep");
    write(&after, "assets/keep.bin", b"keep");

    let patch_root = root.join("patch");
    let patch = Patch::write_between(&before, &after, &patch_root).expect("writes");
    assert_eq!(patch.removed, vec!["assets/old.bin"]);

    patch.apply(&patch_root, &before).expect("applies");
    assert!(!before.join("assets/old.bin").exists());
    assert_eq!(contents(&before), contents(&after));
}

/// A patch for another build is refused rather than applied over it.
#[test]
fn a_patch_for_another_build_is_refused() {
    let root = scratch("wrong-base");
    let before = root.join("before");
    let after = root.join("after");
    write(&before, "manifest.json", b"{\"v\":1}");
    write(&after, "manifest.json", b"{\"v\":2}");

    let patch_root = root.join("patch");
    let patch = Patch::write_between(&before, &after, &patch_root).expect("writes");

    // Somebody else's build: the same shape, different content.
    let other = root.join("other");
    write(&other, "manifest.json", b"{\"v\":9}");
    let error = patch.apply(&patch_root, &other).expect_err("wrong build");
    assert!(matches!(error, AssetError::Patch(_)), "{error:?}");
    assert_eq!(
        fs::read(other.join("manifest.json")).expect("read"),
        b"{\"v\":9}",
        "the refused apply wrote anyway"
    );
}

/// A blob whose bytes do not match its digest is refused, and nothing is written.
///
/// This is the promise `BUILD_AND_ASSETS.md §6.2` makes about a corrupted download: detected,
/// and the full build fetched instead. It is only a promise if the detection precedes the
/// damage, so the test checks the target is untouched.
#[test]
fn a_corrupted_blob_is_refused_before_anything_is_written() {
    let root = scratch("corrupt");
    let before = root.join("before");
    let after = root.join("after");
    write(&before, "manifest.json", b"{\"v\":1}");
    write(&before, "scripts/main.velac", b"old");
    write(&after, "manifest.json", b"{\"v\":2}");
    write(&after, "scripts/main.velac", b"new");

    let patch_root = root.join("patch");
    let patch = Patch::write_between(&before, &after, &patch_root).expect("writes");

    // Corrupt one blob, leaving its name — a truncated download.
    let blob = patch_root.join("blobs");
    let name = fs::read_dir(&blob)
        .expect("read blobs")
        .flatten()
        .map(|entry| entry.path())
        .next()
        .expect("at least one blob");
    fs::write(&name, b"not what was hashed").expect("corrupt the blob");

    let error = patch.apply(&patch_root, &before).expect_err("corrupted");
    assert!(
        error.to_string().contains("does not match the digest"),
        "{error}"
    );
    assert_eq!(
        contents(&before),
        std::collections::BTreeMap::from([
            ("manifest.json".to_string(), b"{\"v\":1}".to_vec()),
            ("scripts/main.velac".to_string(), b"old".to_vec()),
        ]),
        "a failed apply left the bundle half-patched"
    );
}

/// A patch's index survives the trip through its own file.
#[test]
fn a_patch_index_round_trips() {
    let root = scratch("round-trip");
    let before = root.join("before");
    let after = root.join("after");
    write(&before, "a.json", b"1");
    write(&after, "a.json", b"2");

    let patch_root = root.join("patch");
    let written = Patch::write_between(&before, &after, &patch_root).expect("writes");
    assert_eq!(Patch::read(&patch_root).expect("reads"), written);
}

/// Two builds with the same files are the same build, however they were produced.
#[test]
fn identity_is_about_contents() {
    let root = scratch("identity");
    let first = root.join("first");
    let second = root.join("second");
    write(&first, "a.json", b"one");
    write(&first, "dir/b.json", b"two");
    write(&second, "dir/b.json", b"two");
    write(&second, "a.json", b"one");

    assert_eq!(
        identity_of(&first).expect("first"),
        identity_of(&second).expect("second")
    );

    write(&second, "a.json", b"different");
    assert_ne!(
        identity_of(&first).expect("first"),
        identity_of(&second).expect("second")
    );
}

/// A build that did not change needs no patch, and the acceptance bar is met for a text change.
///
/// `VISION.md §5`: a text-only change must produce a patch under 5% of the bundle. The fixture
/// is shaped like a real one — a large asset that does not change and a small script that does —
/// because a bundle with nothing in it would make the ratio meaningless.
#[test]
fn a_text_change_is_a_small_patch() {
    let root = scratch("text-change");
    let before = root.join("before");
    let after = root.join("after");

    let bulk = vec![0u8; 200_000];
    for dir in [&before, &after] {
        write(dir, "assets/art/forest.bin", &bulk);
        write(dir, "assets/art/room.bin", &bulk);
        write(dir, "assets/audio/theme.bin", &bulk);
        write(dir, "manifest.json", b"{\"assets\":[]}");
    }
    write(
        &before,
        "scripts/main.velac",
        b"say \"The rain had stopped.\"",
    );
    write(
        &after,
        "scripts/main.velac",
        b"say \"The rain had stopped!\"",
    );

    let patch_root = root.join("patch");
    let patch = Patch::write_between(&before, &after, &patch_root).expect("writes");

    assert_eq!(patch.entries.len(), 1, "{:?}", patch.entries);
    let bundle = 3 * bulk.len() as u64 + 64;
    let size = patch.size(&patch_root).expect("measures");
    assert!(
        size * 20 < bundle,
        "a text change produced {size} bytes of patch for a {bundle}-byte bundle"
    );
}
