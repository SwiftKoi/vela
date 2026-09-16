//! An action: the call a screen writes, and the declaration that says it exists.

use std::fmt;

use crate::widgets::schema::PropDecl;

use super::builtin::BUILTIN;

/// One action a screen asks for, as written: `open_screen(settings)`.
///
/// The *instance* an [`ActionDecl`] describes. A screen does not mutate state; it produces a
/// description of a mutation (`SCREENS.md §7`), and this is that description — a name and its
/// arguments as written. Arguments stay strings because resolving them (a label, a variable, a
/// screen name) is the runtime's job, and a tree that half-resolved them would be a tree that
/// cannot be re-read.
///
/// An `Action` is also a *value*: a screen may take one as a parameter and hand it to a widget, which
/// is what lets a caller supply the answer (`SCREENS.md §2.1`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Action {
    /// The action's name, e.g. `open_screen`.
    pub name: String,
    /// Its arguments, as written: `settings`, `forest.confession`.
    pub args: Vec<String>,
}

impl Action {
    /// An action call.
    #[must_use]
    pub fn new(name: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            name: name.into(),
            args,
        }
    }

    /// The first argument, if any.
    #[must_use]
    pub fn first(&self) -> Option<&str> {
        self.args.first().map(String::as_str)
    }
}

impl fmt::Display for Action {
    /// Renders the call the way a screen wrote it, for a log or a trace.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", self.name, self.args.join(", "))
    }
}

/// One action a screen can take.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ActionDecl {
    /// The name a screen writes.
    pub name: &'static str,
    /// What it takes, in order.
    pub args: &'static [PropDecl],
    /// What it does, for the docs and for a hover.
    pub doc: &'static str,
    /// Whether the runtime acts on it yet.
    ///
    /// Most of the vocabulary is a *name* before it is a behaviour: `preference` and `file_page` are
    /// the screen language's words for systems the later milestones build. Saying which is which is
    /// the difference between a declared vocabulary and a claim that a button does something — and a
    /// reader of the reference, or of a hover, gets the same answer `vela check` would not otherwise
    /// give.
    pub dispatched: bool,
}

impl ActionDecl {
    /// How many arguments it takes.
    #[must_use]
    pub fn arity(&self) -> usize {
        self.args.len()
    }

    /// The argument names, for a suggestion.
    #[must_use]
    pub fn arg_names(&self) -> Vec<&'static str> {
        self.args.iter().map(|arg| arg.name).collect()
    }

    /// The call as a screen writes it: `jump(label)`.
    ///
    /// One spelling, used by the reference page's heading, by hover, and by the anchor a link into
    /// either is built from — so a link an editor produces lands on the heading it was written for.
    #[must_use]
    pub fn signature(&self) -> String {
        format!("{}({})", self.name, self.arg_names().join(", "))
    }

    /// One sentence: what it does, and whether the runtime acts on it yet.
    ///
    /// The same sentence `vela doc` prints and a hover shows — the pattern `Widget::summary` follows,
    /// and for the same reason: a reference and an editor describing one action differently is how a
    /// reader learns to trust neither.
    #[must_use]
    pub fn summary(&self) -> String {
        if self.dispatched {
            return self.doc.to_string();
        }
        format!(
            "{} Declared, not dispatched yet: the runtime does not act on it.",
            self.doc
        )
    }
}

/// The actions a project can take.
#[derive(Clone, Debug)]
pub struct ActionRegistry {
    actions: Vec<ActionDecl>,
}

impl Default for ActionRegistry {
    fn default() -> Self {
        Self::builtin()
    }
}

impl ActionRegistry {
    /// The default set, with no plugins.
    #[must_use]
    pub fn builtin() -> Self {
        Self {
            actions: BUILTIN.to_vec(),
        }
    }

    /// An empty registry.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            actions: Vec::new(),
        }
    }

    /// Adds an action, replacing any of the same name in place.
    pub fn register(&mut self, action: ActionDecl) {
        if let Some(existing) = self.actions.iter_mut().find(|a| a.name == action.name) {
            *existing = action;
            return;
        }
        self.actions.push(action);
    }

    /// Looks an action up.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ActionDecl> {
        self.actions.iter().find(|action| action.name == name)
    }

    /// Every name, in registration order.
    #[must_use]
    pub fn names(&self) -> Vec<&'static str> {
        self.actions.iter().map(|action| action.name).collect()
    }

    /// How many are registered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.actions.len()
    }

    /// Whether none are.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    /// The closest registered name to `name`, for a `did you mean`.
    #[must_use]
    pub fn closest(&self, name: &str) -> Option<&'static str> {
        self.names()
            .into_iter()
            .map(|candidate| (vela_diag::edit_distance(name, candidate), candidate))
            .filter(|(distance, _)| *distance <= (name.chars().count() / 3).clamp(1, 3))
            .min_by_key(|(distance, _)| *distance)
            .map(|(_, candidate)| candidate)
    }
}
