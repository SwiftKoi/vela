//! The actions every project starts with (`SCREENS.md §7`).
//!
//! A vocabulary, and only partly a set of behaviours. `dispatched` says which is which, and the
//! registry is honest about the rest rather than implying that every word a screen writes does
//! something: `preference` and `file_page` are this language's words for systems the later milestones
//! build, named now because a screen that uses one has to *check* now.
//!
//! Ren'Py's `ShowMenu`, `Start`, and `MainMenu` are deliberately absent, because each is already a
//! word this language has: a menu is a screen (`open_screen`) and the beginning is a label (`jump`).

use crate::widgets::PropType;

use super::decl::ActionDecl;

/// The action that writes a setting (`RUNTIME.md §2.1`).
///
/// A constant for the reason [`SET_SCREEN_VARIABLE`] is one: the checker holds the first argument to
/// the settings vocabulary (`vela-ui::settings`) while the runtime is what performs the write, and a
/// literal in each place would let one of them be renamed without the other.
pub const PREFERENCE: &str = "preference";

/// The action that flips a boolean setting, for the control that cannot read it.
///
/// Ren'Py spells this as the *value* — `Preference("skip", "toggle")` — and a value word is a second
/// vocabulary inside an argument that only the runtime could check. An action is the shape this
/// language uses for a word that does something (`SCREENS.md §7`), and a checkbox is what it exists
/// for: the button flips the setting without a screen ever reading one.
pub const TOGGLE_PREFERENCE: &str = "toggle_preference";

/// The action that writes one of a screen's *own* variables (`SCREENS.md §2.5`).
///
/// A constant because two crates have to agree about the string: `vela-ui` holds the written name to
/// the variables the screen declares, and the runtime is what performs the write. A literal in each
/// place would be a checker that validates one name while the runtime acts on another.
///
/// Ren'Py's `SetScreenVariable` in this language's spelling — the same words, because it is the same
/// action: the store belongs to the screen rather than to the world, and `set` is the world's.
pub const SET_SCREEN_VARIABLE: &str = "set_screen_variable";

/// An argument a screen writes, in order.
const fn arg(name: &'static str, ty: PropType, doc: &'static str) -> crate::widgets::PropDecl {
    crate::widgets::PropDecl {
        name,
        ty,
        required: true,
        doc,
    }
}

/// The actions every project starts with.
pub(crate) const BUILTIN: &[ActionDecl] = &[
    ActionDecl {
        name: "jump",
        args: &[arg("label", PropType::Target, "Where to go.")],
        doc: "Transfer to a label, replacing the current one.",
        dispatched: false,
    },
    ActionDecl {
        name: "call",
        args: &[arg("label", PropType::Target, "Where to go.")],
        doc: "Transfer to a label, remembering where to come back to.",
        dispatched: false,
    },
    ActionDecl {
        name: "return",
        args: &[],
        doc: "Return from a `call`.",
        dispatched: false,
    },
    ActionDecl {
        name: "set",
        args: &[
            arg("name", PropType::Word, "The variable to write."),
            arg("value", PropType::Word, "What to write into it."),
        ],
        doc: "Assign a variable.",
        dispatched: false,
    },
    ActionDecl {
        name: "toggle",
        args: &[arg("name", PropType::Word, "The boolean to flip.")],
        doc: "Flip a boolean.",
        dispatched: false,
    },
    ActionDecl {
        name: "play",
        args: &[
            arg("channel", PropType::Word, "Which channel."),
            arg("source", PropType::Asset, "What to play."),
        ],
        doc: "Start a sound.",
        dispatched: false,
    },
    ActionDecl {
        name: "stop",
        args: &[arg("channel", PropType::Word, "Which channel.")],
        doc: "Stop a sound.",
        dispatched: false,
    },
    ActionDecl {
        name: "open_screen",
        args: &[arg("screen", PropType::Target, "Which screen.")],
        doc: "Show a screen above this one.",
        dispatched: true,
    },
    ActionDecl {
        name: "close_screen",
        args: &[],
        doc: "Dismiss the screen this action is in.",
        dispatched: true,
    },
    ActionDecl {
        name: "hide",
        args: &[arg("screen", PropType::Target, "Which screen to dismiss.")],
        doc: "Dismiss a named screen, wherever it sits in the stack.",
        dispatched: true,
    },
    ActionDecl {
        name: "wait",
        args: &[arg("seconds", PropType::Number, "How long.")],
        doc: "Do nothing for a while, without blocking a frame.",
        dispatched: false,
    },
    ActionDecl {
        name: "quit",
        args: &[],
        doc: "Leave the story.",
        dispatched: true,
    },
    ActionDecl {
        name: "quick_save",
        args: &[],
        doc: "Save to the quick slot.",
        dispatched: true,
    },
    ActionDecl {
        name: "quick_load",
        args: &[],
        doc: "Load the quick slot.",
        dispatched: true,
    },
    ActionDecl {
        name: "rollback",
        args: &[],
        doc: "Step back one command, replaying from the nearest snapshot.",
        dispatched: true,
    },
    ActionDecl {
        name: "skip",
        args: &[],
        doc: "Fast-forward until something needs an answer.",
        dispatched: false,
    },
    ActionDecl {
        name: PREFERENCE,
        args: &[
            arg("name", PropType::Word, "Which setting."),
            arg("value", PropType::Value, "What to set it to."),
        ],
        doc: "Change one of the player's settings.",
        dispatched: false,
    },
    ActionDecl {
        name: TOGGLE_PREFERENCE,
        args: &[arg("name", PropType::Word, "Which setting to flip.")],
        doc: "Flip a boolean setting, for a control that cannot read it.",
        dispatched: false,
    },
    ActionDecl {
        name: "file_page",
        args: &[arg("name", PropType::Word, "Which page.")],
        doc: "Show a page of save slots.",
        dispatched: false,
    },
    ActionDecl {
        name: "file_page_previous",
        args: &[],
        doc: "Show the page of slots before this one.",
        dispatched: false,
    },
    ActionDecl {
        name: "file_page_next",
        args: &[],
        doc: "Show the page of slots after this one.",
        dispatched: false,
    },
    ActionDecl {
        name: "file_action",
        args: &[arg("slot", PropType::Number, "Which slot.")],
        doc: "Save into a slot, or load from it, depending on the screen.",
        dispatched: false,
    },
    ActionDecl {
        name: "file_delete",
        args: &[arg("slot", PropType::Number, "Which slot.")],
        doc: "Delete what is in a slot.",
        dispatched: false,
    },
    ActionDecl {
        name: SET_SCREEN_VARIABLE,
        args: &[
            arg("name", PropType::Word, "Which screen variable."),
            arg("value", PropType::Value, "What to write into it."),
        ],
        doc: "Assign a screen's own variable, rather than the world's.",
        dispatched: true,
    },
    ActionDecl {
        name: "language",
        args: &[arg("name", PropType::Word, "Which locale, or `none`.")],
        doc: "Switch the language the story is read in.",
        dispatched: false,
    },
    ActionDecl {
        name: "end_replay",
        args: &[],
        doc: "Leave replay mode.",
        dispatched: false,
    },
    ActionDecl {
        name: "gamepad_calibrate",
        args: &[],
        doc: "Run the joystick calibration.",
        dispatched: false,
    },
];
