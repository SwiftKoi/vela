//! An action: the call a screen writes, and the declaration that says it exists.

use std::fmt;

use crate::value::Value;
use crate::widgets::schema::PropDecl;

use super::builtin::BUILTIN;

/// One action a screen asks for: `open_screen(settings)`.
///
/// The *instance* an [`ActionDecl`] describes. A screen does not mutate state; it produces a
/// description of a mutation (`SCREENS.md §7`), and this is that description — a name and its
/// arguments.
///
/// An argument is what the screen says it is, which is one of two things and both matter. A **name** —
/// `settings`, `forest.confession` — is for the runtime to resolve: a screen cannot know what labels a
/// project has. A **value** is what the screen already had, resolved against its own scope, because only
/// the screen knows its scope and a runtime that re-resolved it could not: `set_screen_variable(device,
/// "mouse")` has to write the string, and `set(trust, trust + 1)` the number, rather than the *words*.
/// A name that is not in scope is a name; that is the whole of the difference (`§2.4`).
///
/// An `Action` is also a *value*: a screen may take one as a parameter and hand it to a widget, which
/// is what lets a caller supply the answer (`SCREENS.md §2.1`).
#[derive(Clone, PartialEq, Debug)]
pub struct Action {
    /// The action's name, e.g. `open_screen`.
    pub name: String,
    /// Its arguments, in order.
    pub args: Vec<Value>,
}

impl Action {
    /// An action call.
    #[must_use]
    pub fn new(name: impl Into<String>, args: Vec<Value>) -> Self {
        Self {
            name: name.into(),
            args,
        }
    }

    /// The first argument's text, when it is a name or a string.
    ///
    /// What every action that takes a target or a variable reads — `open_screen(settings)`,
    /// `hide(notify)`, `set_screen_variable(device, …)` — and `None` for an argument of another kind,
    /// which is a value where a name was wanted.
    #[must_use]
    pub fn first(&self) -> Option<&str> {
        match self.args.first()? {
            Value::Str(text) => Some(text),
            _ => None,
        }
    }
}

impl fmt::Display for Action {
    /// Renders the call the way a screen wrote it, for a log or a trace.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}(", self.name)?;
        for (index, arg) in self.args.iter().enumerate() {
            if index > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", arg.as_text())?;
        }
        write!(f, ")")
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
    /// What may follow the declared arguments, when the call is *another declaration's* shape.
    ///
    /// One action needs it: `open_screen` hands its extra arguments to the screen it opens, whose
    /// parameters this registry cannot know (`SCREENS.md §2.1`). Declared rather than assumed, because
    /// the alternative is a checker that lets any action take any number of arguments — a vocabulary
    /// nobody could hold a call to.
    pub rest: Option<PropDecl>,
}

impl ActionDecl {
    /// How many arguments a call must give at least.
    ///
    /// A minimum rather than the number, because an entry with [`rest`](Self::rest) takes more: what
    /// the extra ones are is the *target's* shape, and only the target knows.
    #[must_use]
    pub fn arity(&self) -> usize {
        self.args.len()
    }

    /// Whether more arguments may follow the declared ones.
    #[must_use]
    pub fn takes_rest(&self) -> bool {
        self.rest.is_some()
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
    /// The trailing ellipsis is an entry with [`rest`](Self::rest): the arguments after the named ones
    /// belong to what it opens, and the reference says whose they are.
    #[must_use]
    pub fn signature(&self) -> String {
        let mut names = self.arg_names();
        if self.takes_rest() {
            names.push("…");
        }
        format!("{}({})", self.name, names.join(", "))
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
