//! The migration chain: ordering, the exact gap it names, and the operations it offers.
//!
//! `RUNTIME.md §6`. These are the highest-risk paths in the engine — they run on the one
//! artifact a player cannot regenerate — so the tests are about the *failure* modes as much
//! as the happy one: a chain that skips a version, a save whose world is only half brought
//! forward, and a step declared out of order.

use vela_replay::{ChainError, Migration, Migrator, ReplayError, SAVE_VERSION, Save, checksum};
use vela_vm::{Snapshot, VmState};
use vela_world::{Value, World};

/// A schema from a one-file source.
fn schema_of(source: &str) -> vela_replay::Schema {
    let parsed = vela_syntax::parse(vela_span::FileId::from_raw(0), source);
    vela_replay::Schema::derive(&parsed.program.items)
}

/// A save at the current version whose world is built by `fill`.
fn save_at_current_version(fill: impl FnOnce(&mut World)) -> Vec<u8> {
    let mut world = World::new();
    fill(&mut world);
    let snapshot = Snapshot {
        world,
        vm: VmState::default(),
        current: None,
    };
    Save::new(snapshot, [0u8; 32], "quick")
        .to_bytes()
        .expect("encode")
}

/// Rewrites a save's version and re-checksums it, to stand in for a build from another time.
fn with_version(bytes: &[u8], version: u16) -> Vec<u8> {
    let mut out = bytes.to_vec();
    out[4..6].copy_from_slice(&version.to_le_bytes());
    let end = out.len() - 8;
    let sum = checksum(&out[..end]);
    out[end..].copy_from_slice(&sum.to_le_bytes());
    out
}

/// A chain applies its steps in order, so a later step sees an earlier one's work.
#[test]
fn a_chain_applies_its_steps_in_order() {
    let migrator = Migrator::new(vec![
        Migration::new(1, 2).rename_field("trust", "affection"),
        Migration::new(2, 3).transform(double_affection),
    ])
    .expect("the chain is contiguous");
    let schema = schema_of("");

    let mut world = World::new();
    world.set("trust", Value::Int(3));
    migrator
        .migrate(&mut world, &schema, 1, 3)
        .expect("every step is present");

    assert_eq!(world.get("trust"), None, "the old name is left behind");
    assert_eq!(
        world.get("affection"),
        Some(&Value::Int(6)),
        "the transform ran on the renamed value, not on `trust`"
    );
}

/// A gap fails with `E7201` naming *that step*, not the whole distance to the engine.
#[test]
fn a_missing_step_names_the_exact_gap() {
    let migrator = Migrator::new(vec![
        Migration::new(1, 2).rename_field("a", "b"),
        Migration::new(3, 4).rename_field("b", "c"),
    ])
    .expect("each step is adjacent; the chain simply has a hole");

    let mut world = World::new();
    let error = migrator
        .migrate(&mut world, &schema_of(""), 1, 4)
        .expect_err("2 → 3 is missing");

    assert_eq!(error.code(), "E7201");
    match error {
        ReplayError::MissingMigration { from, to } => {
            assert_eq!((from, to), (2, 3), "the gap is the step to write");
        }
        other => panic!("expected a missing migration, got {other:?}"),
    }
}

/// A chain that skips a version is rejected when it is built, not when a save needs it.
#[test]
fn a_skipped_version_is_rejected() {
    let error = Migrator::new(vec![Migration::new(1, 3)]).expect_err("1 → 3 skips 2");
    assert!(matches!(error, ChainError::NotAdjacent { from: 1, to: 3 }));
}

/// Two migrations from the same version is ambiguous, and rejected.
#[test]
fn a_duplicate_start_is_rejected() {
    let error = Migrator::new(vec![Migration::new(1, 2), Migration::new(1, 2)])
        .expect_err("two steps claim version 1");
    assert!(matches!(error, ChainError::Duplicate { from: 1 }));
}

/// The order a caller writes steps in does not matter; the version does.
#[test]
fn the_chain_is_ordered_by_version() {
    let migrator = Migrator::new(vec![Migration::new(2, 3), Migration::new(1, 2)])
        .expect("the chain is contiguous");
    assert_eq!(migrator.versions(), vec![1, 2]);
}

/// `add_default` seeds a value only when the current schema declares it.
///
/// A chain is shared by every project, so an unconditional `add_default` would write a field
/// into the worlds of projects whose stories never declared one.
#[test]
fn add_default_respects_the_current_schema() {
    let migration = Migration::new(1, 2).add_default("cap", Value::Int(10));

    let mut world = World::new();
    migration.apply(&mut world, &schema_of("default cap: int = 0\n"));
    assert_eq!(world.get("cap"), Some(&Value::Int(10)));

    let mut other = World::new();
    migration.apply(&mut other, &schema_of("default trust: int = 0\n"));
    assert_eq!(
        other.get("cap"),
        None,
        "a project that does not declare `cap` must not receive it"
    );
}

/// Renaming a field no save ever held is a no-op, not an error.
#[test]
fn renaming_an_absent_field_does_nothing() {
    let migration = Migration::new(1, 2).rename_field("trust", "affection");
    let mut world = World::new();
    world.set("coins", Value::Int(1));
    migration.apply(&mut world, &schema_of(""));
    assert_eq!(world.get("coins"), Some(&Value::Int(1)));
    assert_eq!(world.get("affection"), None);
}

/// The `migration!` macro builds a step with every operation, in the order written.
#[test]
fn the_macro_declares_a_step() {
    let step = macro_step::migration();
    assert_eq!((step.from(), step.to()), (1, 2));

    let mut world = World::new();
    world.set("trust", Value::Int(2));
    step.apply(&mut world, &schema_of("default cap: int = 0\n"));

    assert_eq!(world.get("trust"), None);
    assert_eq!(world.get("affection"), Some(&Value::Int(4)));
    assert_eq!(world.get("cap"), Some(&Value::Int(9)));
}

/// A module written the way a migration file is, so the macro is exercised as it is used.
mod macro_step {
    use vela_replay::migration;
    use vela_world::{Value, World};

    migration! {
        from = 1,
        to = 2,
        rename_field = ("trust", "affection"),
        add_default = ("cap", Value::Int(9)),
        transform = double,
    }

    /// Doubles whatever the rename left behind.
    fn double(world: &mut World) {
        if let Some(Value::Int(value)) = world.get("affection") {
            world.set("affection", Value::Int(value * 2));
        }
    }
}

/// A chain that reaches the current version: the rename this test is about, then steps that
/// rewrite nothing.
///
/// Built here rather than taken from the shipped chain, so the test is about `Save::load`'s
/// version handling and not about whatever the engine's migrations do this release. The
/// trailing steps keep it true as `SAVE_VERSION` moves.
fn chain_to_current() -> Migrator {
    let mut steps = vec![Migration::new(1, 2).rename_field("trust", "affection")];
    for from in 2..SAVE_VERSION {
        steps.push(Migration::new(from, from + 1));
    }
    Migrator::new(steps).expect("the chain is contiguous")
}

/// An older save loads through the chain and comes back at the current version.
#[test]
fn an_older_save_loads_through_the_chain() {
    let bytes = with_version(
        &save_at_current_version(|world| world.set("trust", Value::Int(3))),
        1,
    );
    let migrator = chain_to_current();
    let schema = schema_of("default affection: int = 0\n");

    // At the current version the same bytes are refused, because the chain is not consulted.
    if SAVE_VERSION > 1 {
        let strict = Save::from_bytes(&bytes).expect_err("version 1 is not current");
        assert_eq!(strict.code(), "E7201");
    }

    let loaded = Save::load(&bytes, &migrator, &schema).expect("the chain bridges the gap");
    assert_eq!(
        loaded.header.save_version, SAVE_VERSION,
        "a loaded save is always at the current version"
    );
    assert_eq!(loaded.header.schema_digest, schema.digest());
    assert_eq!(loaded.snapshot.world.get("affection"), Some(&Value::Int(3)));
    assert_eq!(loaded.snapshot.world.get("trust"), None);
}

/// A save the chain cannot reach fails with `E7201`, naming the step that is missing.
#[test]
fn a_save_the_chain_cannot_reach_names_the_gap() {
    let bytes = with_version(&save_at_current_version(|_| {}), 0);
    let error = Save::load(&bytes, &Migrator::default(), &schema_of(""))
        .expect_err("nothing bridges 0 → 1");
    assert_eq!(error.code(), "E7201");
    match error {
        ReplayError::MissingMigration { from, to } => assert_eq!((from, to), (0, 1)),
        other => panic!("expected a missing migration, got {other:?}"),
    }
}

/// The step this build ships is present for every version below the current one.
///
/// This is the check that makes "a save from build N loads in build N+1" a fact about the
/// tree rather than a claim in a document: a bump to `SAVE_VERSION` without the matching
/// migration fails here.
#[test]
fn the_shipped_chain_covers_every_version() {
    let expected: Vec<u16> = (1..SAVE_VERSION).collect();
    assert_eq!(vela_replay::chain().versions(), expected);
}

/// Doubles an integer value, guarding on its presence.
fn double_affection(world: &mut World) {
    if let Some(Value::Int(value)) = world.get("affection") {
        world.set("affection", Value::Int(value * 2));
    }
}
