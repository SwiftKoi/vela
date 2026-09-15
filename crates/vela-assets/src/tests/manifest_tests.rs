//! The manifest: that it round-trips, that it is canonical, and that a version it does not know
//! is refused.

use std::collections::BTreeMap;

use crate::digest::Digest;
use crate::manifest::{Artifact, Asset, Manifest, Variant};

/// A manifest with one asset of everything the schema has, as an import would leave it: the
/// bundle descriptor fields unset, because an import does not know what project it belongs to.
fn manifest() -> Manifest {
    let mut manifest = Manifest::new();
    manifest.assets = vec![Asset {
        id: "art.forest".to_string(),
        source: "art/forest.json".to_string(),
        digest: Digest::of(b"{}"),
        artifacts: vec![Artifact {
            path: "art/forest.json".to_string(),
            digest: Digest::of(b"{}"),
            kind: "data".to_string(),
            variants: vec![Variant {
                target: "web".to_string(),
                digest: Digest::of(b"[]"),
            }],
        }],
        size: BTreeMap::from([("web".to_string(), 12)]),
    }];
    manifest
}

/// The manifest survives the trip through its own file.
#[test]
fn a_manifest_round_trips() {
    let original = manifest();
    let text = original.to_json().expect("writes");
    assert!(text.ends_with("}\n"), "a manifest file ends in a newline");
    assert_eq!(Manifest::from_json(&text).expect("reads"), original);
}

/// The same manifest writes the same bytes, which is what makes two builds comparable at all.
#[test]
fn writing_is_canonical() {
    let first = manifest().to_json().expect("writes");
    let second = manifest().to_json().expect("writes");
    assert_eq!(first, second);
}

/// Empty collections are omitted rather than written as `[]`, so a version-1 manifest from
/// before targets existed is byte-identical to one written now.
#[test]
fn empty_variants_and_sizes_are_omitted() {
    let mut bare = manifest();
    bare.assets[0].artifacts[0].variants.clear();
    bare.assets[0].size.clear();
    let text = bare.to_json().expect("writes");
    assert!(!text.contains("variants"), "{text}");
    assert!(!text.contains("size"), "{text}");
}

/// The bundle descriptor a build fills in round-trips, and is *omitted* when unset — which is
/// what keeps a manifest written by a bare import byte-identical to one written before the
/// fields existed.
#[test]
fn the_bundle_descriptor_round_trips_and_is_omitted_when_unset() {
    let mut described = manifest();
    let bare = described.to_json().expect("writes");
    for field in ["\"name\"", "\"entry\"", "\"images\""] {
        assert!(
            !bare.contains(field),
            "{field} was written for an import:\n{bare}"
        );
    }

    described.set_project(Some("forest".to_string()), "main.start");
    described
        .images
        .insert("bg.forest".to_string(), "art/forest.json".to_string());

    let text = described.to_json().expect("writes");
    let read = Manifest::from_json(&text).expect("reads");
    assert_eq!(read.entry.as_deref(), Some("main.start"));
    assert_eq!(read.name.as_deref(), Some("forest"));
    assert_eq!(read.image("bg.forest"), Some("art/forest.json"));
    assert_eq!(
        read.artifact_for_source("art/forest.json"),
        Some("art/forest.json")
    );
}

/// A version this build does not write is refused, not read as if the unknown fields were
/// missing.
#[test]
fn a_future_version_is_refused() {
    let text = manifest()
        .to_json()
        .expect("writes")
        .replace("\"manifest_version\": 1", "\"manifest_version\": 2");
    let error = Manifest::from_json(&text).expect_err("version 2 is not this build's");
    assert!(error.to_string().contains("version 2"), "{error}");
}

/// An asset is found by id or by source, which is the two questions the pipeline asks.
#[test]
fn an_asset_is_found_by_id_and_by_source() {
    let manifest = manifest();
    assert!(manifest.asset("art.forest").is_some());
    assert!(manifest.asset("art.pond").is_none());
    assert!(manifest.has_source("art/forest.json"));
    assert!(!manifest.has_source("art/pond.json"));
}
