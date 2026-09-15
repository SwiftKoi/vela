//! The asset golden: an assets tree in, a manifest out (`CONVENTIONS.md §4.4` step 4).
//!
//! A manifest is the build's identity, so what it pins is not the *bytes* of an artifact —
//! those would change with every compressor — but the shape and the digests: which id a source
//! became, which artifact it produced, and what each hashed to. A change to an importer that
//! alters any of those shows up here as a diff, which is the point.
//!
//! `cargo xtask bless` regenerates every `.expected`. CI never blesses.

use std::fs;
use std::path::{Path, PathBuf};

use vela_assets::{ImporterRegistry, import_tree};

/// Whether to rewrite goldens rather than compare against them.
fn blessing() -> bool {
    std::env::var_os("VELA_BLESS").is_some()
}

/// Where the samples and their expected manifests live.
fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/assets")
}

/// Every sample, which is every subdirectory, in a stable order.
fn samples(dir: &Path) -> Vec<PathBuf> {
    let entries =
        fs::read_dir(dir).unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    found.sort();
    found
}

#[test]
fn goldens_match() {
    let dir = golden_dir();
    let mut checked = 0usize;
    let mut differences = Vec::new();

    for sample in samples(&dir) {
        let built = import_tree(&sample, &ImporterRegistry::builtin())
            .unwrap_or_else(|e| panic!("{}: {e}", sample.display()));
        let actual = built.manifest.to_json().expect("writes");

        let name = sample
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let expected_path = dir.join(format!("{name}.expected"));

        if blessing() {
            fs::write(&expected_path, &actual).expect("write golden");
        } else {
            let expected = fs::read_to_string(&expected_path).unwrap_or_default();
            if actual != expected {
                differences.push(format!(
                    "{name}\n--- expected ---\n{expected}--- actual ---\n{actual}"
                ));
            }
        }
        checked += 1;
    }

    assert!(checked > 0, "no asset samples in {}", dir.display());
    assert!(
        differences.is_empty(),
        "{} asset golden(s) differ — run `cargo xtask bless`, then review the diff\n\n{}",
        differences.len(),
        differences.join("\n")
    );
}
