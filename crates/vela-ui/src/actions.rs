//! The action registry.
//!
//! `SCREENS.md §7`: *"the action set is a registry. Adding an action is a registry entry, not
//! a UI-core edit."* The same shape as widgets, and for the same reason — an action set is a
//! thing that grows with what a project wants to do, and a `match` on an action name would be
//! a core file every new action has to edit.
//!
//! Interaction produces **typed** actions and a screen never mutates state directly, which is
//! what keeps rendering one-directional: `paint` reads, actions write, and nothing does both.
//! `set(trust, trust + 1)` is a description of a mutation that the runtime performs, not a
//! mutation that a button performs.

use std::fmt;

use crate::widgets::schema::PropDecl;

/// One action a screen asks for, as written: `open_screen(settings)`.
///
/// The *instance* an [`ActionDecl`] describes. A screen does not mutate state; it produces a
/// description of a mutation (`SCREENS.md §7`), and this is that description — a name and its
/// arguments as written. Arguments stay strings because resolving them (a label, a variable, a
/// screen name) is the runtime's job, and a tree that half-resolved them would be a tree that
/// cannot be re-read.
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
}

/// An argument that is a bare identifier.
const fn arg(name: &'static str, ty: crate::widgets::PropType, doc: &'static str) -> PropDecl {
    PropDecl {
        name,
        ty,
        required: true,
        doc,
    }
}

/// The actions every project starts with (`SCREENS.md §7`).
pub const BUILTIN: &[ActionDecl] = &[
    ActionDecl {
        name: "jump",
        args: &[arg(
            "label",
            crate::widgets::PropType::Target,
            "Where to go.",
        )],
        doc: "Transfer to a label, replacing the current one.",
    },
    ActionDecl {
        name: "call",
        args: &[arg(
            "label",
            crate::widgets::PropType::Target,
            "Where to go.",
        )],
        doc: "Transfer to a label, remembering where to come back to.",
    },
    ActionDecl {
        name: "return",
        args: &[],
        doc: "Return from a `call`.",
    },
    ActionDecl {
        name: "set",
        args: &[
            arg(
                "name",
                crate::widgets::PropType::Word,
                "The variable to write.",
            ),
            arg(
                "value",
                crate::widgets::PropType::Word,
                "What to write into it.",
            ),
        ],
        doc: "Assign a variable.",
    },
    ActionDecl {
        name: "toggle",
        args: &[arg(
            "name",
            crate::widgets::PropType::Word,
            "The boolean to flip.",
        )],
        doc: "Flip a boolean.",
    },
    ActionDecl {
        name: "play",
        args: &[
            arg("channel", crate::widgets::PropType::Word, "Which channel."),
            arg("source", crate::widgets::PropType::Asset, "What to play."),
        ],
        doc: "Start a sound.",
    },
    ActionDecl {
        name: "stop",
        args: &[arg(
            "channel",
            crate::widgets::PropType::Word,
            "Which channel.",
        )],
        doc: "Stop a sound.",
    },
    ActionDecl {
        name: "open_screen",
        args: &[arg(
            "screen",
            crate::widgets::PropType::Target,
            "Which screen.",
        )],
        doc: "Show a screen above this one.",
    },
    ActionDecl {
        name: "close_screen",
        args: &[],
        doc: "Dismiss the screen this action is in.",
    },
    ActionDecl {
        name: "wait",
        args: &[arg(
            "seconds",
            crate::widgets::PropType::Number,
            "How long.",
        )],
        doc: "Do nothing for a while, without blocking a frame.",
    },
    ActionDecl {
        name: "quit",
        args: &[],
        doc: "Leave the story.",
    },
    ActionDecl {
        name: "quick_save",
        args: &[],
        doc: "Save to the quick slot.",
    },
    ActionDecl {
        name: "quick_load",
        args: &[],
        doc: "Load the quick slot.",
    },
];

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
