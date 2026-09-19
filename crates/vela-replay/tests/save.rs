//! Save files: the container, the checksum, and the version discipline.
//!
//! `RUNTIME.md §5` and §6.2. A save is the one artifact a player cannot regenerate, so these
//! are about the ways a save can go wrong — truncated, from another build, against another
//! schema — and about the bytes being reproducible.

use std::collections::BTreeMap;

use vela_replay::{ReplayError, SAVE_VERSION, Save, checksum};
use vela_vm::{Snapshot, VmState};
use vela_world::{Key, Rng, Tick, Value, World};

/// A world with one of everything a save has to carry.
fn world() -> World {
    let mut world = World::new();
    world.set("trust", Value::Int(3));
    world.set("name", Value::Str("Eileen".to_string()));
    world.set("awake", Value::Bool(true));
    world.set("rest", Value::Float(1.5));
    world.set("flags", Value::List(vec![Value::Bool(true), Value::None]));
    world.set(
        "scores",
        Value::Map(BTreeMap::from([(Key::Str("a".to_string()), Value::Int(1))])),
    );
    world.set(
        "hero",
        Value::Struct {
            name: "Hero".to_string(),
            fields: vec![("hp".to_string(), Value::Int(9))],
        },
    );
    world.set(
        "route",
        Value::Enum {
            name: "Route".to_string(),
            variant: "Forest".to_string(),
            fields: vec![Value::Int(2)],
        },
    );
    world.rng = Rng::seeded(42);
    world.clock = Tick(7);
    world.call_stack = vec!["start".to_string(), "outside".to_string()];
    world.log_cursor = 3;
    world.scene.show("bg.room");
    world.audio.play("music", "rain.ogg", true);
    world
}

/// A snapshot of the world above, with an empty machine.
fn snapshot() -> Snapshot {
    Snapshot {
        world: world(),
        vm: VmState::default(),
        current: None,
    }
}

/// A save of that snapshot, at version 1.
fn save() -> Save {
    let mut save = Save::new(snapshot(), [0u8; 32], "quick");
    save.log_len = 3;
    save
}

/// Rewrites a save's version and re-checksums it, to stand in for another build's file.
fn with_version(bytes: &[u8], version: u16) -> Vec<u8> {
    let mut out = bytes.to_vec();
    out[4..6].copy_from_slice(&version.to_le_bytes());
    let end = out.len() - 8;
    let sum = checksum(&out[..end]);
    out[end..].copy_from_slice(&sum.to_le_bytes());
    out
}

/// Every kind of value, the scene, the audio, the RNG, and the clock survive a round trip.
#[test]
fn a_save_round_trips() {
    let original = save();
    let bytes = original.to_bytes().expect("encode");
    let decoded = Save::from_bytes(&bytes).expect("decode");

    assert_eq!(decoded.header, original.header);
    assert_eq!(decoded.snapshot, original.snapshot);
    assert_eq!(decoded.log_len, original.log_len);
}

/// The same save encodes to the same bytes — a save is compared and diffed.
#[test]
fn the_bytes_are_stable() {
    let first = save().to_bytes().expect("encode");
    let second = save().to_bytes().expect("encode");
    assert_eq!(first, second);
}

/// A truncated file is caught by the checksum, not by a length that was cut off with it.
#[test]
fn a_truncated_save_is_caught_by_the_checksum() {
    let bytes = save().to_bytes().expect("encode");
    let truncated = &bytes[..bytes.len() - 12];

    let error = Save::from_bytes(truncated).expect_err("truncation must be detected");
    assert!(matches!(error, ReplayError::Corrupt { .. }), "{error:?}");
    assert_eq!(error.code(), "E7203");
}

/// A single altered byte is caught too.
#[test]
fn a_tampered_byte_is_caught() {
    let mut bytes = save().to_bytes().expect("encode");
    let index = bytes.len() - 20;
    bytes[index] ^= 0xff;

    let error = Save::from_bytes(&bytes).expect_err("tampering must be detected");
    assert_eq!(error.code(), "E7203");
}

/// A save from a newer build is refused, and says so.
#[test]
fn a_newer_save_is_refused() {
    let bytes = with_version(&save().to_bytes().expect("encode"), SAVE_VERSION + 1);

    let error = Save::from_bytes(&bytes).expect_err("a newer save must be refused");
    assert!(
        matches!(error, ReplayError::FromTheFuture { .. }),
        "{error:?}"
    );
    assert_eq!(error.code(), "E7202");
}

/// A save from an older build names the version gap it cannot cross.
#[test]
fn an_older_save_names_the_gap() {
    let bytes = with_version(&save().to_bytes().expect("encode"), 0);

    let error = Save::from_bytes(&bytes).expect_err("an older save needs a migration");
    assert_eq!(error.code(), "E7201");
    match error {
        ReplayError::MissingMigration { from, to } => {
            assert_eq!(
                (from, to),
                (0, 1),
                "the gap is one step, not the whole distance"
            );
        }
        other => panic!("expected a missing migration, got {other:?}"),
    }
}

/// A file that is not a save is rejected before anything is parsed.
#[test]
fn a_file_that_is_not_a_save_is_refused() {
    let error = Save::from_bytes(b"this is a text file, not a save").expect_err("not a save");
    assert!(
        matches!(error, ReplayError::NotRecognised { .. }),
        "{error:?}"
    );
    assert_eq!(error.code(), "E7203");
}

/// The schema a save was written against is checked, and names both digests when it differs.
#[test]
fn a_schema_mismatch_is_refused() {
    let save = save();
    let schema = schema_of("default trust: int = 0\n");
    let error = save.expect_schema(&schema).expect_err("the digests differ");
    assert_eq!(error.code(), "E7204");
    match error {
        ReplayError::SchemaMismatch { saved, current } => {
            assert_eq!(saved.len(), 64, "a 256-bit digest as hex");
            assert_ne!(saved, current);
        }
        other => panic!("expected a schema mismatch, got {other:?}"),
    }
}

/// A save whose float cannot be represented is refused rather than written as `null`.
#[test]
fn a_non_finite_float_is_refused() {
    let mut world = World::new();
    world.set("broken", Value::Float(f64::NAN));
    let snapshot = Snapshot {
        world,
        vm: VmState::default(),
        current: None,
    };
    let save = Save::new(snapshot, [0u8; 32], "quick");

    let error = save.to_bytes().expect_err("NaN cannot be saved");
    match error {
        ReplayError::NonFiniteFloat { name } => assert_eq!(name, "broken"),
        other => panic!("expected a non-finite float, got {other:?}"),
    }
}

/// Writing is atomic: the file is readable after, and a second write replaces it.
#[test]
fn writing_replaces_the_slot() {
    let dir = std::env::temp_dir().join(format!("vela-save-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create dir");
    let path = dir.join("quick.velasave");

    save().write_atomic(&path).expect("write");
    let read = Save::read(&path).expect("read");
    assert_eq!(read.snapshot, save().snapshot);

    let mut second = save();
    second.snapshot.world.set("trust", Value::Int(99));
    second.write_atomic(&path).expect("overwrite");
    assert_eq!(
        Save::read(&path).expect("read").snapshot.world.get("trust"),
        Some(&Value::Int(99)),
        "the second write did not replace the first"
    );

    std::fs::remove_dir_all(&dir).expect("clean up");
}

/// The schema, derived from a source, for the mismatch test.
fn schema_of(source: &str) -> vela_replay::Schema {
    let parsed = vela_syntax::parse(vela_span::FileId::from_raw(0), source);
    vela_replay::Schema::derive(&parsed.program.items)
}
