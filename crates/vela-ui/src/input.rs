//! The semantic actions a screen may answer (`SCREENS.md §11`).
//!
//! Input is abstracted to *meaning*, not to buttons: the window resolves a device event to one of
//! these and hands it on, so a screen never asks which key was pressed and a binding profile can move
//! the key without touching the screen. `vela_host` is the crate that owns the device table; this is
//! the vocabulary the names come from, kept here because the party that has to *check* a screen for a
//! misspelled action is the screen checker, and it sits below the host.
//!
//! That leaves two lists of the same names — this one and `vela_host::Action` — which is the sort of
//! duplication the repository normally refuses. It is accepted here for one reason: the crate that
//! would own the single list, `vela-host`, depends on `winit`, and a checker that pulled a windowing
//! library in to validate a name would be a worse trade. A test in `vela-cli`, the one crate that can
//! see both, asserts the two lists are the same, so they cannot drift apart in CI.

/// The action names the engine delivers.
pub const BUILTIN: &[&str] = &[
    "advance",
    "skip",
    "rollback",
    "menu_up",
    "menu_down",
    "confirm",
    "cancel",
    "screenshot",
    "quit",
];

/// The vocabulary a `key` line's name is checked against.
///
/// Supplied to the checker rather than looked up inside it, the same shape the widget and action
/// registries already have: the caller that knows the target's input profile is the one that can say
/// what its window will deliver.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SemanticActions {
    names: Vec<String>,
}

impl Default for SemanticActions {
    fn default() -> Self {
        Self::builtin()
    }
}

impl SemanticActions {
    /// The set the engine ships, mirroring `vela_host::Action` (`SCREENS.md §11`).
    #[must_use]
    pub fn builtin() -> Self {
        Self {
            names: BUILTIN.iter().map(|name| (*name).to_string()).collect(),
        }
    }

    /// A vocabulary of these names.
    ///
    /// Order is kept, because that is what a completion list and a doc page show — a set that
    /// iterated unpredictably could not be printed or golden-tested.
    #[must_use]
    pub fn new(names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            names: names.into_iter().map(Into::into).collect(),
        }
    }

    /// Whether the host delivers an action by this name.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.names.iter().any(|known| known == name)
    }

    /// The names, in order.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.names.iter().map(String::as_str).collect()
    }

    /// The nearest name, if one is close enough to be worth offering.
    ///
    /// A wrong suggestion is worse than none, so the bar is the same one every other suggestion in the
    /// language uses: within a third of the name's length, and never more than three edits.
    #[must_use]
    pub fn closest(&self, name: &str) -> Option<&str> {
        self.names
            .iter()
            .map(|known| (vela_diag::edit_distance(name, known), known.as_str()))
            .filter(|(distance, _)| *distance <= (name.chars().count() / 3).clamp(1, 3))
            .min_by_key(|(distance, _)| *distance)
            .map(|(_, known)| known)
    }
}
