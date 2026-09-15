//! World state.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

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
/// (`RUNTIME.md §2`).
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct World {
    /// The values declared by `default`, which are the save schema.
    ///
    /// Ordered, not hashed: iteration order is observable through serialization, and a
    /// `HashMap` here would make two runs of the same story produce different bytes.
    defaults: BTreeMap<String, Value>,
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
    #[must_use]
    pub fn snapshot(&self) -> Self {
        self.clone()
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
