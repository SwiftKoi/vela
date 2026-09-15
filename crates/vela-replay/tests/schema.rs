//! The save schema: what contributes to it, and when its digest changes.
//!
//! `RUNTIME.md §2`: the schema is derived from `default`, `struct`, and `enum` declarations.
//! The digest is what a load compares, so the tests that matter are about *stability* — the
//! same shape hashing the same across builds, and a changed shape hashing differently.

use vela_replay::Schema;
use vela_span::FileId;

/// Derives a schema from a one-file source.
fn schema(source: &str) -> Schema {
    let parsed = vela_syntax::parse(FileId::from_raw(0), source);
    assert!(
        parsed.diagnostics.is_empty(),
        "the fixture must parse: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| d.message.clone())
            .collect::<Vec<_>>()
    );
    Schema::derive(&parsed.program.items)
}

/// Every kind of declaration contributes an entry.
#[test]
fn the_schema_holds_defaults_structs_and_enums() {
    let schema = schema(
        "default trust: int = 0\n\ndefault name: str = \"\"\n\nstruct Hero:\n    hp: int\n\nenum Route:\n    Forest\n    Town(kind: int)\n",
    );
    assert_eq!(schema.entries().len(), 4, "{:?}", schema.entries());
    assert!(schema.canonical().contains("default trust: int"));
    assert!(schema.canonical().contains("struct Hero { hp: int }"));
    assert!(
        schema
            .canonical()
            .contains("enum Route { Forest, Town(int) }")
    );
}

/// The same source hashes the same — twice, and across a re-derivation.
#[test]
fn the_digest_is_stable() {
    let source = "default trust: int = 0\n";
    assert_eq!(schema(source).digest(), schema(source).digest());
}

/// Adding a `default` changes the schema, and so the digest.
#[test]
fn adding_a_default_changes_the_digest() {
    let before = schema("default trust: int = 0\n");
    let after = schema("default trust: int = 0\ndefault affection: int = 0\n");
    assert_ne!(before.digest(), after.digest());
    assert_eq!(after.entries().len(), 2);
}

/// Changing a struct field changes the digest.
#[test]
fn changing_a_struct_field_changes_the_digest() {
    let before = schema("struct Hero:\n    hp: int\n");
    let after = schema("struct Hero:\n    hp: float\n");
    assert_ne!(before.digest(), after.digest());
}

/// Declaration order does not change the schema: a `default` moved in the source saves the
/// same state, so it must hash the same.
#[test]
fn order_does_not_change_the_digest() {
    let first = schema("default a: int = 0\ndefault b: int = 0\n");
    let second = schema("default b: int = 0\ndefault a: int = 0\n");
    assert_eq!(first.digest(), second.digest());
}

/// A source with nothing to save has an empty schema, and still a digest.
#[test]
fn an_empty_schema_is_empty() {
    let schema = schema("label start:\n    return\n");
    assert!(schema.is_empty());
    assert_eq!(schema.canonical(), "");
    assert_eq!(schema.digest(), vela_replay::digest(b""));
}
