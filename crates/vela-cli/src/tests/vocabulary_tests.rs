//! The two vocabularies a button's action can be lowered through, held together.
//!
//! `vela-cli` is the only crate that can see both: the migration is rank 8 and cannot depend on the
//! screen layer, and the screen layer has no reason to know about Ren'Py. So the assertion lives here,
//! the way `ui_tests`'s `the_semantic_actions_match_the_hosts` holds the input map to the host's list.
//!
//! Why it matters: the migration lowers an action by looking its Vela name up in `known_actions()`. A
//! name that is *not* in the list is dropped and reported as "Vela has no name for it" — a press that
//! loses its action, in a project that still checks clean. That is exactly what happened when
//! `replace_screen` was added: the registry had it, the table's mapping named it, and the list did not,
//! so the sample's whole `navigation` lost every button and the report called `ShowMenu` unsupported.
//!
//! The check is **one-way** on purpose. Every name the migration lowers has to be one the language has —
//! a name the migration invents would be a project that does not check, which is the invariant. But the
//! list is allowed to be shorter than the registry, because an action whose *arguments* the migration
//! cannot supply yet must not be lowered at all: `file_action(slot)` needs a `$ slot = i + 1` line the
//! migration reports as uncarried, and lowering it turned the sample's `check` from clean to three
//! errors the first time this list was completed. `KNOWN_ACTIONS` records the rule.

/// Every action the migration can lower is one the registry has.
#[test]
fn the_migration_lowers_only_actions_the_registry_has() {
    let registry = vela_ui::ActionRegistry::builtin();

    for name in vela_migrate::known_actions() {
        assert!(
            registry.get(name).is_some(),
            "the migration would lower `{name}`, which no build has: a project that migrated to it \
             would not check"
        );
    }
}
