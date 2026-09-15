//! The append-only save corpus (`RUNTIME.md §6.3`).
//!
//! A save is the one artifact a player cannot regenerate, and migrations run on saves written
//! by builds that no longer exist. So the corpus keeps a *real* save file for every version
//! the format has ever had, and CI loads each one into the current build and asserts the
//! world it produces.
//!
//! Two rules make this a check rather than a ceremony:
//!
//! * **Every version must be present.** The assertion at the end is over `1..=SAVE_VERSION`,
//!   so deleting a historical file — or bumping the version without adding its save — fails
//!   here rather than in a player's hands. That is the "explicit maintainer decision" of §6.3,
//!   made something the test can see.
//! * **A `.expected` is a diff, not a rubber stamp.** `cargo xtask bless` writes it, and CI
//!   fails on any unblessed change, so a migration that quietly alters an old save shows up in
//!   review as the diff it is.
//!
//! `cargo xtask bless` also *seeds* the save for the current version when it is missing, from
//! the fixture below. Historical files are never rewritten: the seeder only ever adds the
//! version this build writes.

mod common;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use common::compile;
use vela_replay::{SAVE_VERSION, Save, Schema, chain};
use vela_vm::Session;

/// The story the current build's save is written from.
///
/// The shape it declares is the *current* schema, which is what a migrated save is checked
/// against. A copy of the previous version's story sits beside it in the corpus, so the change
/// a migration bridges is readable rather than inferred.
const FIXTURE: &str = include_str!("../../../tests/golden/saves/fixture.vela");

/// Whether to write goldens rather than compare against them.
fn blessing() -> bool {
    std::env::var_os("VELA_BLESS").is_some()
}

/// The corpus directory.
fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/saves")
}

/// Every corpus save, in a stable order.
fn saves(dir: &Path) -> Vec<PathBuf> {
    let entries =
        fs::read_dir(dir).unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "velasave"))
        .collect();
    paths.sort();
    paths
}

/// The version a corpus file is named for: `v<version>_<name>.velasave`.
fn version_of(path: &Path) -> u16 {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    name.strip_prefix('v')
        .and_then(|rest| rest.split('_').next())
        .and_then(|digits| digits.parse().ok())
        .unwrap_or_else(|| {
            panic!(
                "{}: name a corpus save `v<version>_<name>.velasave`",
                path.display()
            )
        })
}

/// The schema of a one-file source.
fn schema_of(source: &str) -> Schema {
    let parsed = vela_syntax::parse(vela_span::FileId::from_raw(0), source);
    Schema::derive(&parsed.program.items)
}

/// The world, as a canonical, diffable rendering — the whole thing, not one field.
fn rendered(save: &Save) -> String {
    let json = serde_json::to_string_pretty(&save.snapshot.world).expect("the world serializes");
    format!("{json}\n")
}

/// Writes the current version's save if the corpus has none.
///
/// The save is produced the way a player's is: the fixture is compiled, run to its first
/// suspension, and snapshotted. Fabricating the bytes would test the loader against a writer
/// that does not exist.
fn seed_current(dir: &Path) {
    let path = dir.join(format!("v{SAVE_VERSION}_fixture.velasave"));
    if path.exists() {
        return;
    }
    let module = compile("fixture", FIXTURE);
    let mut session = Session::start(&module, "start").expect("the fixture starts");
    session.advance();
    let schema = schema_of(FIXTURE);
    let save = Save::new(session.snapshot(), schema.digest(), "fixture");
    fs::create_dir_all(dir).expect("the corpus directory");
    save.write_atomic(&path).expect("write the save");
    eprintln!("seeded {}", path.display());
}

/// Every version's save loads into this build and produces the world it is expected to.
#[test]
fn the_corpus_loads_into_the_current_build() {
    let dir = corpus_dir();
    if blessing() {
        fs::create_dir_all(&dir).expect("the corpus directory");
        seed_current(&dir);
    }

    let schema = schema_of(FIXTURE);
    let migrator = chain();
    let mut versions = BTreeSet::new();
    let mut differences = Vec::new();

    let files = saves(&dir);
    assert!(!files.is_empty(), "no corpus saves in {}", dir.display());

    for path in &files {
        versions.insert(version_of(path));
        let bytes = fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let save = Save::load(&bytes, &migrator, &schema)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(
            save.header.save_version,
            SAVE_VERSION,
            "{}: a loaded save is always at the current version",
            path.display()
        );

        let actual = rendered(&save);
        let expected_path = path.with_extension("expected");
        if blessing() {
            fs::write(&expected_path, &actual).expect("write golden");
            continue;
        }
        let expected = fs::read_to_string(&expected_path).unwrap_or_default();
        if actual != expected {
            differences.push(format!(
                "{}\n--- expected ---\n{expected}--- actual ---\n{actual}",
                path.display()
            ));
        }
    }

    assert!(
        differences.is_empty(),
        "{} corpus save(s) differ — run `cargo xtask bless`, then review the diff\n\n{}",
        differences.len(),
        differences.join("\n")
    );

    let expected_versions: BTreeSet<u16> = (1..=SAVE_VERSION).collect();
    assert_eq!(
        versions, expected_versions,
        "the corpus must hold a save for every version (RUNTIME.md §6.3); \
         `cargo xtask bless` seeds the current one"
    );
}
