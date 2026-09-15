//! Importing: selection, normalization, ordering, and the ways a build should refuse.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::AssetError;
use crate::importers::{ImportRequest, Importer, ImporterRegistry, Output};
use crate::{Digest, import_tree};

/// A directory of its own per test, removed first so a rerun starts clean.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vela-assets-{name}-{}", std::process::id()));
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

/// An importer that claims a magic prefix, so the selection rule can be tested on its own
/// rather than through whichever format happens to ship with a signature.
struct Claimed {
    magic: &'static [u8],
    extensions: &'static [&'static str],
}

impl Importer for Claimed {
    fn kind(&self) -> &'static str {
        "claimed"
    }

    fn extensions(&self) -> &'static [&'static str] {
        self.extensions
    }

    fn probe(&self, bytes: &[u8]) -> bool {
        bytes.starts_with(self.magic)
    }

    fn import(&self, request: &ImportRequest<'_>) -> Result<Vec<Output>, AssetError> {
        Ok(vec![Output {
            path: format!("{}.out", request.stem()),
            kind: self.kind(),
            bytes: request.bytes.to_vec(),
        }])
    }
}

/// Magic bytes win over the extension: a `.png` that is really a JPEG is imported as what it
/// is, which is the whole reason the order is specified.
#[test]
fn magic_bytes_are_checked_before_the_extension() {
    let mut registry = ImporterRegistry::new();
    registry.register(Box::new(Claimed {
        magic: b"\xff\xd8\xff",
        extensions: &["jpg"],
    }));

    // The bytes say JPEG, the name says PNG.
    let chosen = registry
        .select("art/photo.png", b"\xff\xd8\xff\xe0rest")
        .expect("the magic bytes match");
    assert_eq!(chosen.kind(), "claimed");

    // The bytes say nothing, so the extension decides.
    let by_name = registry
        .select("art/photo.jpg", b"not-a-jpeg")
        .expect("the extension matches");
    assert_eq!(by_name.kind(), "claimed");

    assert!(registry.select("art/photo.png", b"not-a-jpeg").is_none());
}

/// JSON that differs only in formatting is one artifact, which is what makes the digest an
/// identity rather than a record of how the file was typed.
#[test]
fn json_is_normalized() {
    let dir = scratch("normalize-json");
    write(&dir, "a.json", b"{ \"b\": 1, \"a\": 2 }");
    write(&dir, "b.json", b"{\"a\":2,\n\"b\":1}");

    let built = import_tree(&dir, &ImporterRegistry::builtin()).expect("imports");
    let bytes: Vec<&Vec<u8>> = built.artifacts.iter().map(|(_, bytes)| bytes).collect();
    assert_eq!(bytes.len(), 2);
    assert_eq!(bytes[0], bytes[1], "two spellings became two artifacts");
    assert_eq!(bytes[0], br#"{"a":2,"b":1}"#);
}

/// TOML becomes the same JSON, including the datetime JSON has no type for.
#[test]
fn toml_becomes_json() {
    let dir = scratch("toml");
    write(
        &dir,
        "conf.toml",
        b"title = \"A\"\nwhen = 2024-01-02T03:04:05Z\nsize = 3\nratio = 0.5\n",
    );

    let built = import_tree(&dir, &ImporterRegistry::builtin()).expect("imports");
    let (path, bytes) = &built.artifacts[0];
    assert_eq!(path, "conf.json", "the artifact takes the runtime's format");
    assert_eq!(
        String::from_utf8_lossy(bytes),
        r#"{"ratio":0.5,"size":3,"title":"A","when":"2024-01-02T03:04:05Z"}"#
    );
}

/// The tree is imported in a fixed order, so the manifest does not depend on the filesystem.
#[test]
fn the_manifest_is_ordered() {
    let dir = scratch("ordered");
    write(&dir, "zulu.json", b"{}");
    write(&dir, "alpha.json", b"{}");
    write(&dir, "art/mid.json", b"{}");

    let built = import_tree(&dir, &ImporterRegistry::builtin()).expect("imports");
    let ids: Vec<&str> = built
        .manifest
        .assets
        .iter()
        .map(|asset| asset.id.as_str())
        .collect();
    assert_eq!(ids, vec!["alpha", "art.mid", "zulu"]);
}

/// Two builds of one source are byte-identical — the reproducibility requirement, at the
/// size a unit test can hold.
#[test]
fn two_builds_are_identical() {
    let dir = scratch("reproducible");
    write(&dir, "art/forest.toml", b"name = \"forest\"\n");
    write(&dir, "ui/theme.json", b"{ \"bg\": \"#10121a\" }");

    let first = import_tree(&dir, &ImporterRegistry::builtin()).expect("imports");
    let second = import_tree(&dir, &ImporterRegistry::builtin()).expect("imports");

    assert_eq!(first.artifacts, second.artifacts);
    assert_eq!(
        first.manifest.to_json().expect("writes"),
        second.manifest.to_json().expect("writes")
    );
}

/// The source digest is of the *source* bytes, not the artifact, so a format change does not
/// make every asset look new.
#[test]
fn the_asset_digest_is_of_the_source() {
    let dir = scratch("source-digest");
    write(&dir, "a.json", b"{ \"b\": 1, \"a\": 2 }");

    let built = import_tree(&dir, &ImporterRegistry::builtin()).expect("imports");
    let asset = &built.manifest.assets[0];
    assert_eq!(asset.digest, Digest::of(b"{ \"b\": 1, \"a\": 2 }"));
    assert_ne!(
        asset.digest, asset.artifacts[0].digest,
        "the source and its artifact differ, or this test proves nothing"
    );
}

/// A file nothing claims is a report, not a silent skip: an asset dropped from a build is a
/// blank rectangle in front of a playtester.
#[test]
fn an_unclaimed_file_is_refused() {
    let dir = scratch("unclaimed");
    write(&dir, "mystery.bin", b"\x00\x01\x02");

    let error = import_tree(&dir, &ImporterRegistry::builtin()).expect_err("nothing claims it");
    assert!(matches!(error, AssetError::Unsupported { .. }), "{error:?}");
    assert!(error.to_string().contains("mystery.bin"), "{error}");
}

/// A malformed source is the importer's failure, and it names the file.
#[test]
fn a_malformed_source_is_refused() {
    let dir = scratch("malformed");
    write(&dir, "broken.json", b"{ not json");

    let error = import_tree(&dir, &ImporterRegistry::builtin()).expect_err("not JSON");
    match error {
        AssetError::Import { ref path, kind, .. } => {
            assert_eq!(kind, "data");
            assert!(path.ends_with("broken.json"), "{path:?}");
        }
        other => panic!("expected an import failure, got {other:?}"),
    }
}

/// Two sources that drop to the same id are refused, and both are named — the alternative is
/// whichever the walk reached second quietly winning.
#[test]
fn two_sources_cannot_share_an_id() {
    let dir = scratch("duplicate");
    write(&dir, "art/forest.json", b"{}");
    write(&dir, "art/forest.toml", b"");

    let error =
        import_tree(&dir, &ImporterRegistry::builtin()).expect_err("both want `art.forest`");
    match error {
        AssetError::DuplicateId { ref id, .. } => assert_eq!(id, "art.forest"),
        other => panic!("expected a duplicate id, got {other:?}"),
    }
    assert!(error.to_string().contains("forest.json"), "{error}");
    assert!(error.to_string().contains("forest.toml"), "{error}");
}

/// Hidden files are skipped, so an editor's swap file does not fail a build.
#[test]
fn hidden_files_are_skipped() {
    let dir = scratch("hidden");
    write(&dir, "kept.json", b"{}");
    write(&dir, ".hidden.json", b"{}");
    write(&dir, ".git/objects.json", b"{}");

    let built = import_tree(&dir, &ImporterRegistry::builtin()).expect("imports");
    assert_eq!(built.manifest.assets.len(), 1);
    assert_eq!(built.manifest.assets[0].id, "kept");
}

/// An empty tree is a build with nothing in it, not an error: the asset half is optional.
#[test]
fn an_empty_tree_is_an_empty_manifest() {
    let dir = scratch("empty");
    let built = import_tree(&dir, &ImporterRegistry::builtin()).expect("imports nothing");
    assert!(built.manifest.assets.is_empty());
    assert!(built.artifacts.is_empty());
}
