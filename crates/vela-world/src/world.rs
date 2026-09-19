//! World state.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::preferences::Preferences;
use crate::rng::Rng;
use crate::stage::{AudioState, SceneState};
use crate::value::Value;

/// Injected time, in ticks. Never read from the operating system (`ARCHITECTURE.md §4`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default, Serialize, Deserialize)]
pub struct Tick(pub u64);

/// The mutable state of a running story.
///
/// Plain typed data with no host pointers, which is what makes it snapshottable and
/// serializable — rollback, saves, and replay all fall out of that one property
/// (`RUNTIME.md §2`). One field is not the story's: `preferences` belong to the *player*, and
/// `RUNTIME.md §2.1` is the rule that keeps them out of everything a story reproduces.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct World {
    /// The values declared by `default`, which are the save schema.
    ///
    /// Ordered, not hashed: iteration order is observable through serialization, and a
    /// `HashMap` here would make two runs of the same story produce different bytes.
    defaults: BTreeMap<String, Value>,
    /// The player's settings — the second lifetime (`RUNTIME.md §2.1`).
    ///
    /// Skipped by the serializer, so no save carries one, and left out of [`World::snapshot`], so no
    /// rollback restores one. What a restore does instead is take them from its caller — `vela-vm`'s
    /// `Session::restore` — because a setting is the player's and no saved state may speak for them.
    #[serde(skip)]
    pub preferences: Preferences,
    /// The single deterministic generator, advanced only by the `rand` effects.
    pub rng: Rng,
    /// Time, advanced only by an explicit tick.
    pub clock: Tick,
    /// What is on stage.
    pub scene: SceneState,
    /// What is playing.
    pub audio: AudioState,
    /// Where execution is, for a save to restore.
    ///
    /// Names rather than frame ids: a save written by one build must load in another, and a
    /// label still exists after a recompile while an index into the function table may not.
    pub call_stack: Vec<String>,
    /// How far into the input log this world has been driven.
    pub log_cursor: u64,
}

impl World {
    /// An empty world.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A default's current value.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.defaults.get(name)
    }

    /// Sets a default.
    pub fn set(&mut self, name: impl Into<String>, value: Value) {
        self.defaults.insert(name.into(), value);
    }

    /// Removes a default, returning what it held.
    ///
    /// Used by a migration that renames a field (`RUNTIME.md §6.1`): the value moves to a
    /// new name, which means leaving the old one behind. A save's world only holds the
    /// defaults a story has actually touched, so removing one that is absent is not an
    /// error.
    pub fn remove(&mut self, name: &str) -> Option<Value> {
        self.defaults.remove(name)
    }

    /// Every default, in name order.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Value)> {
        self.defaults.iter()
    }

    /// A copy, for comparing the state at two points or from two executions.
    ///
    /// The player's settings are not in it: they are not state a run reproduces, so two runs of one
    /// story with different settings compare equal — and a snapshot, which is copied from this, is
    /// not a place a rollback could undo a setting from (`RUNTIME.md §2.1`).
    #[must_use]
    pub fn snapshot(&self) -> Self {
        let mut story = self.clone();
        story.preferences = Preferences::default();
        story
    }
}

impl std::fmt::Display for World {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("{")?;
        for (index, (name, value)) in self.defaults.iter().enumerate() {
            if index > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{name}: {value}")?;
        }
        f.write_str("}")
    }
}
