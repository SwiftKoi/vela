//! The settings file: where a player's choices live between sessions (`RUNTIME.md §2.1`).
//!
//! Two rules, and each is a way a settings file could quietly lose what it holds. It is **versioned**
//! like a save, so the day the store's names change there is a step to write and a gap that names itself
//! (`E7201`) rather than a file that resets to the defaults in silence. And it is **refused before it is
//! parsed** when the magic or the checksum is wrong, the way a save is.
//!
//! `tests/golden/settings/` keeps a real file for every version the format has had: the corpus rule
//! `RUNTIME.md §6.3` states for saves, for the same reason — a version bump with no fixture and no step
//! is a bug, and this is where it fails rather than in a player's hands.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use vela_replay::settings::{self, FILE_NAME, MAGIC};
use vela_replay::{ReplayError, SETTINGS_VERSION, Settings};
use vela_world::{Preferences, Value};

/// Whether to write goldens rather than compare against them.
fn blessing() -> bool {
    std::env::var_os("VELA_BLESS").is_some()
}

/// The corpus directory.
fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/settings")
}

/// A store with one setting of each kind the vocabulary has, so a codec change is visible.
fn chosen() -> Preferences {
    let mut preferences = Preferences::new();
    preferences.set("text_speed", Value::Int(30));
    preferences.set("skip_unseen", Value::Bool(true));
    preferences.set("display_mode", Value::Str("fullscreen".to_string()));
    preferences
}

/// A file written and read back is the same settings, at the current version.
#[test]
fn a_settings_file_round_trips() {
    let settings = Settings::new(chosen());
    let bytes = settings.to_bytes().expect("encode");

    let read = Settings::from_bytes(&bytes).expect("decode");
    assert_eq!(read.preferences, chosen());
    assert_eq!(read.version, SETTINGS_VERSION);
    assert_eq!(
        Settings::version_of(&bytes).expect("a header"),
        SETTINGS_VERSION
    );
}

/// A file is written where a player's saves are, under the name the caller looks it up by.
#[test]
fn a_settings_file_is_written_atomically() {
    let dir = std::env::temp_dir().join(format!("vela-settings-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create the directory");
    let path = dir.join(FILE_NAME);

    Settings::new(chosen()).write_atomic(&path).expect("write");
    let read = Settings::read(&path).expect("read");
    assert_eq!(read.preferences, chosen());

    let _ = fs::remove_dir_all(&dir);
}

/// A version the build cannot reach names the gap rather than resetting the store.
#[test]
fn a_version_this_build_cannot_reach_names_the_gap() {
    // From the future: refused, and it says which version wrote it.
    let mut ahead = Settings::new(chosen());
    ahead.version = SETTINGS_VERSION + 1;
    let error = Settings::from_bytes(&ahead.to_bytes().expect("encode")).expect_err("too new");
    assert!(
        matches!(error, ReplayError::FromTheFuture { .. }),
        "{error:?}"
    );
    assert_eq!(error.code(), "E7202");

    // From before version 1 there is nothing, so the *missing step* is what is named — which is what a
    // player sees instead of preferences silently becoming the defaults.
    let mut ancient = Settings::new(chosen());
    ancient.version = 0;
    let error = Settings::from_bytes(&ancient.to_bytes().expect("encode")).expect_err("too old");
    assert!(
        matches!(error, ReplayError::MissingMigration { from: 0, to: 1 }),
        "{error:?}"
    );
    assert_eq!(error.code(), "E7201");
    assert!(
        error
            .to_string()
            .contains("no migration from version 0 to 1"),
        "{error}"
    );
}

/// A file that is not one, or is damaged, is refused before anything is parsed.
#[test]
fn a_file_that_is_not_a_settings_file_is_refused() {
    let error = Settings::from_bytes(b"this is a text file, not settings").expect_err("not ours");
    assert!(
        matches!(error, ReplayError::NotRecognised { .. }),
        "{error:?}"
    );
    assert_eq!(error.code(), "E7203");
    let short = Settings::version_of(&[0x56, 0x50, 0x52, 0x45]).expect_err("too short");
    assert!(
        matches!(short, ReplayError::NotRecognised { .. }),
        "{short:?}"
    );

    // A flipped byte in the payload is caught by the checksum, which covers the bytes.
    let mut bytes = Settings::new(chosen()).to_bytes().expect("encode");
    let at = bytes.len() - 12;
    bytes[at] ^= 0xff;
    let error = Settings::from_bytes(&bytes).expect_err("damaged");
    assert!(matches!(error, ReplayError::Corrupt { .. }), "{error:?}");
}

/// A float the payload cannot hold is refused rather than written as `null`.
#[test]
fn a_value_the_payload_cannot_hold_is_refused() {
    let mut preferences = Preferences::new();
    preferences.set("bad", Value::Float(f64::NAN));
    let error = Settings::new(preferences)
        .to_bytes()
        .expect_err("non-finite");
    assert!(
        matches!(error, ReplayError::NonFiniteFloat { .. }),
        "{error:?}"
    );
}

/// Every version has a real file, and each one loads into this build.
///
/// The corpus rule for saves, applied to the settings file (`RUNTIME.md §6.3`): deleting a historical
/// file — or bumping the version without adding its fixture *and* the step that bridges it — fails here.
#[test]
fn the_corpus_loads_into_the_current_build() {
    let dir = corpus_dir();
    if blessing() {
        fs::create_dir_all(&dir).expect("the corpus directory");
        let path = dir.join(format!("v{SETTINGS_VERSION}_fixture.velaprefs"));
        if !path.exists() {
            Settings::new(chosen())
                .write_atomic(&path)
                .expect("seed the fixture");
            eprintln!("seeded {}", path.display());
        }
    }

    let entries =
        fs::read_dir(&dir).unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "velaprefs"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no corpus files in {}", dir.display());

    let mut versions = BTreeSet::new();
    for path in &paths {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let version: u16 = name
            .strip_prefix('v')
            .and_then(|rest| rest.split('_').next())
            .and_then(|digits| digits.parse().ok())
            .unwrap_or_else(|| {
                panic!(
                    "{}: name a corpus file `v<version>_<name>`, got `{name}`",
                    path.display()
                )
            });
        versions.insert(version);

        let bytes = fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(
            &bytes[..4],
            MAGIC,
            "{}: a corpus file starts with the format's magic",
            path.display()
        );
        let read =
            Settings::from_bytes(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(
            read.version,
            SETTINGS_VERSION,
            "{}: a loaded file is always at the current version",
            path.display()
        );
    }

    let expected: BTreeSet<u16> = (1..=SETTINGS_VERSION).collect();
    assert_eq!(
        versions, expected,
        "the corpus must hold a file for every version; `cargo xtask bless` seeds the current one"
    );
}

/// A version step is one line of Rust over a flat map, and the chain is what says which versions have
/// one — so the shape is asserted here rather than discovered by a player.
#[test]
fn the_chain_is_ordered_and_adjacent() {
    let steps = settings::chain();
    for pair in steps.windows(2) {
        assert!(pair[0].from < pair[1].from, "the chain is ordered");
    }
    for step in &steps {
        assert_eq!(
            step.to,
            step.from + 1,
            "a step bridges exactly one version: {} -> {}",
            step.from,
            step.to
        );
    }
    // And a store at the current version needs no step at all.
    let mut preferences = chosen();
    settings::migrate(&mut preferences, SETTINGS_VERSION, SETTINGS_VERSION).expect("nothing to do");
    assert_eq!(preferences, chosen());
}
