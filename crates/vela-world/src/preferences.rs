//! The player's settings, which are state with a lifetime of their own (`RUNTIME.md §2.1`).
//!
//! A `default` belongs to the **playthrough**: it is in every save, every rollback, and every replay.
//! A preference belongs to the **player**: it is in none of them, and it is still there after a load.
//! That is the whole rule, and this type exists because the two look identical otherwise — both are a
//! name and a value — so the difference has to live somewhere a reader will find it.
//!
//! It is a store rather than a struct of fields because the *vocabulary* is the interface's
//! (`SCREENS.md §7`'s `preference` action, and the settings screens built over it): this type is the
//! state behind that vocabulary, and a setting this build has never heard of must survive a round trip
//! through the settings file rather than be dropped as unknown.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::value::Value;

/// What the player has chosen, by name.
///
/// Ordered, not hashed: iteration order is observable — the settings file is written from this, and a
/// `HashMap` would make two players with the same settings produce different bytes
/// (`CONVENTIONS.md §2.2`, enforced by `xtask check-determinism`).
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct Preferences {
    values: BTreeMap<String, Value>,
}

impl Preferences {
    /// An empty store: a player who has chosen nothing, which is every player at first.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// What a setting is set to, if it is set at all.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.values.get(name)
    }

    /// Sets a setting.
    pub fn set(&mut self, name: impl Into<String>, value: Value) {
        self.values.insert(name.into(), value);
    }

    /// Every setting, in name order.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Value)> {
        self.values.iter()
    }

    /// How many settings there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}
