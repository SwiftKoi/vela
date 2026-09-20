//! The settings a screen can write: the vocabulary behind `preference` (`SCREENS.md §7`).
//!
//! `RUNTIME.md §2.1` says what a setting *is* — the player's state, the second lifetime, in no save
//! and undone by no rollback. This is the list of them, and it is a table for the reason the actions
//! and the widgets are: a setting is added by adding a row, and the checker, the reference, and the
//! screens all read the same one. A name a screen writes is in here or it is `E5020`; a value is one
//! the setting takes or it is `E5021`.
//!
//! **Measured, not invented.** The list is what `the_question`'s own settings screen asks for —
//! twelve `Preference(…)` uses, nine distinct settings
//! (`docs/roadmap/M12.2-game-interface.md`, "Found during implementation") — minus the ones whose
//! system is not built: the volumes and the mute toggle are M12.3's audio, and the migration reports
//! those by name rather than writing a button that does nothing. One entry comes from the *file*
//! screens instead of the settings screen: the page, which Ren'Py keeps in `persistent._file_page` —
//! the same lifetime, so the same store.
//!
//! **The `read` flag is the honest half of a vocabulary that runs ahead of its systems.** Most of them
//! are read by nothing yet: the screens that draw one and the transport that obeys one are this
//! milestone's item 4, and the page is the exception — the file actions are its system, and they are
//! here. Saying so per setting is the same shape `ActionDecl::dispatched` has, and it is what
//! stops "Vela has a text speed" from reading as "Vela types your dialogue out".

use vela_world::{Preferences, Value};

use crate::actions::{Action, PREFERENCE, TOGGLE_PREFERENCE};
use crate::value::Value as ScreenValue;

/// What a setting holds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SettingTy {
    /// A boolean: `preference(skip_unseen, true)`.
    Bool,
    /// A number: `preference(text_speed, 30)`.
    Number,
    /// One of a fixed set of words: `preference(display_mode, fullscreen)`.
    Choice(&'static [&'static str]),
}

impl SettingTy {
    /// The words this type takes, for a reference page and a suggestion.
    #[must_use]
    pub fn describe(self) -> String {
        match self {
            Self::Bool => "true or false".to_string(),
            Self::Number => "a number".to_string(),
            Self::Choice(words) => words.join(", "),
        }
    }

    /// Whether a *written* value is one this setting takes.
    ///
    /// The checker asks this about the values it can see whole — a literal. An expression is the
    /// runtime's to answer, and `SCREENS.md §7` says so rather than implying the check is total.
    #[must_use]
    pub fn takes(self, written: &str) -> bool {
        match self {
            Self::Bool => matches!(written, "true" | "false"),
            Self::Number => written.parse::<f64>().is_ok(),
            Self::Choice(words) => words.contains(&written),
        }
    }
}

/// One setting a screen can write.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SettingDecl {
    /// The name a screen writes: `preference(text_speed, 30)`.
    pub name: &'static str,
    /// What it holds.
    pub ty: SettingTy,
    /// What it is before the player changes it, as Vela writes it.
    pub default: &'static str,
    /// What it means, for the docs and for a hover.
    pub doc: &'static str,
    /// Whether the engine reads it yet.
    ///
    /// A setting has a system behind it or it is a button that does nothing, so which is which is
    /// part of the declaration rather than something a reader has to find out by pressing it.
    pub read: bool,
}

impl SettingDecl {
    /// One sentence: what it means, and whether anything reads it yet.
    ///
    /// The pattern `Widget::summary` and `ActionDecl::summary` follow, and for the same reason: a
    /// reference page, a hover, and the report a migration writes all describe one setting the same
    /// way, or a reader learns to trust none of them.
    #[must_use]
    pub fn summary(&self) -> String {
        if self.read {
            return self.doc.to_string();
        }
        format!(
            "{} Declared, not read yet: nothing in the engine obeys it.",
            self.doc
        )
    }
}

/// Every setting this build knows, in the order a reference page lists them.
///
/// The order is the reading order of a settings screen rather than alphabetical, because that is what
/// it is: text presentation first, then auto-forward, then skipping, then display.
pub const SETTINGS: &[SettingDecl] = &[
    SettingDecl {
        name: "text_speed",
        ty: SettingTy::Number,
        default: "0",
        doc: "How fast dialogue types out, in characters per second. `0` is instant, which is what \
              most readers want and what a keyboard-only player needs.",
        read: false,
    },
    SettingDecl {
        name: "auto_forward",
        ty: SettingTy::Bool,
        default: "false",
        doc: "Whether the story advances on its own, without a press.",
        read: false,
    },
    SettingDecl {
        name: "auto_forward_time",
        ty: SettingTy::Number,
        default: "15",
        doc: "How many seconds a line waits before auto-forward moves on.",
        read: false,
    },
    SettingDecl {
        name: "skip_unseen",
        ty: SettingTy::Bool,
        default: "false",
        doc: "Whether skipping continues through text the player has not read yet.",
        read: false,
    },
    SettingDecl {
        name: "skip_after_choices",
        ty: SettingTy::Bool,
        default: "true",
        doc: "Whether skipping survives a menu the player answered.",
        read: false,
    },
    SettingDecl {
        name: "transitions",
        ty: SettingTy::Bool,
        default: "true",
        doc: "Whether transitions are shown at all.",
        read: false,
    },
    SettingDecl {
        name: "display_mode",
        ty: SettingTy::Choice(&["window", "fullscreen"]),
        default: "window",
        doc: "How the game's window is shown.",
        read: false,
    },
    SettingDecl {
        name: PAGE,
        ty: SettingTy::Number,
        default: "1",
        doc: "Which page of save slots a file screen is on, counting from one. Ren'Py keeps it in \
              `persistent._file_page` for the same reason it is here: which page the player was \
              looking at is the player's state, not the save's (`RUNTIME.md §2.1`). A page's slots \
              are the ones whose names start with it (`vela_replay::slot_name`).",
        read: true,
    },
];

/// The setting that holds the page a file screen is on.
///
/// Public because three actions move it and a screen reads it: `file_page(p)` and the pair that step
/// are [`write`]'s, and `setting("file_page")` is the read the screens use.
pub const PAGE: &str = "file_page";

/// Which page a file screen is on, counting from one.
///
/// A page is a *position*, so a value that cannot be one — absent, zero, negative, or something a newer
/// build wrote that this one cannot read — answers the first page rather than being propagated. That is
/// the same rule `RUNTIME.md §2.1` gives a store with no vocabulary: hold what a newer build wrote,
/// read what this one understands.
#[must_use]
pub fn page(preferences: &Preferences) -> u32 {
    match preferences.get(PAGE) {
        Some(vela_world::Value::Int(number)) => u32::try_from(*number).unwrap_or(1).max(1),
        Some(vela_world::Value::Float(number)) if *number >= 1.0 => *number as u32,
        _ => 1,
    }
}

/// The page after this one.
#[must_use]
pub fn next_page(preferences: &Preferences) -> u32 {
    page(preferences) + 1
}

/// The page before this one, never below the first.
///
/// Ren'Py's `FilePagePrevious` is *insensitive* on the first page rather than wrapping — a screen says
/// so with `enable_if`, which is why the button greys out — and this clamps anyway, because a disabled
/// control is not a guarantee.
#[must_use]
pub fn previous_page(preferences: &Preferences) -> u32 {
    page(preferences).saturating_sub(1).max(1)
}

impl SettingDecl {
    /// The declaration of this name.
    #[must_use]
    pub fn named(name: &str) -> Option<&'static SettingDecl> {
        SETTINGS.iter().find(|setting| setting.name == name)
    }

    /// Every name, in the order a settings screen lists them.
    #[must_use]
    pub fn names() -> Vec<&'static str> {
        SETTINGS.iter().map(|setting| setting.name).collect()
    }

    /// The closest name to `name`, for a `did you mean`.
    ///
    /// The same rule the action registry uses: only suggest when the candidate is genuinely close,
    /// because a wrong suggestion is worse than none.
    #[must_use]
    pub fn closest(name: &str) -> Option<&'static str> {
        Self::names()
            .into_iter()
            .map(|candidate| (vela_diag::edit_distance(name, candidate), candidate))
            .filter(|(distance, _)| *distance <= (name.chars().count() / 3).clamp(1, 3))
            .min_by_key(|(distance, _)| *distance)
            .map(|(_, candidate)| candidate)
    }
}

/// Whether a control's action writes what the store already holds (`SCREENS.md §5.1`).
///
/// A `preference("display_mode", "window")` is the current choice when the setting *is* `window`, and a
/// `toggle_preference("skip_unseen")` when it is on — read the way [`write`] flips one, and the way a
/// screen's `setting("…")` answers, so an unchosen setting is its declaration: the option that is the
/// default is the option that draws as on.
///
/// Asked here rather than in the painter because it is a question about the *store*, and the two actions
/// are this module's vocabulary. What it answers is a fact about one layout, so the caller stores it on the
/// node: a press that changes a setting re-lays, which is what makes the answer current.
#[must_use]
pub fn is_chosen(preferences: &Preferences, action: &Action) -> bool {
    let Some(name) = action.first() else {
        return false;
    };
    let Some(setting) = SettingDecl::named(name) else {
        return false;
    };
    if action.name == TOGGLE_PREFERENCE {
        return match preferences.get(name) {
            Some(Value::Bool(stored)) => *stored,
            _ => setting.default == "true",
        };
    }
    if action.name != PREFERENCE {
        return false;
    }
    let (Some(want), stored) = (action.args.get(1), preferences.get(name)) else {
        return false;
    };
    let Some(want) = world_value(want) else {
        return false;
    };
    match stored {
        Some(stored) => matches(setting, stored, &want),
        // The declaration, in the type it declares rather than as the text it is written as: comparing a
        // button's `0` against the string `"0"` would leave the default option of every numeric setting
        // unmarked, which is the one option the engine knows the answer to.
        None => matches(setting, &declared(setting), &want),
    }
}

/// Whether a stored value is the one a control writes, for a setting of this type.
///
/// Numbers compare as numbers, because a setting declared `Number` is one type while the store can hold a
/// number two ways: a press writes a float (`preference("text_speed", 30)`) and a value the world put
/// there may be an int, and the player reading a settings screen cannot tell those apart — so neither can
/// the mark.
fn matches(setting: &SettingDecl, stored: &Value, want: &Value) -> bool {
    match (setting.ty, number(stored), number(want)) {
        (SettingTy::Number, Some(stored), Some(want)) => stored == want,
        _ => stored == want,
    }
}

/// A value as a number, when it is one of the two the store can hold.
fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Int(number) => Some(*number as f64),
        Value::Float(number) => Some(*number),
        _ => None,
    }
}

/// A declaration's default, as the store would hold it: the text it is written as, in its own type.
fn declared(setting: &SettingDecl) -> Value {
    match setting.ty {
        SettingTy::Bool => Value::Bool(setting.default == "true"),
        SettingTy::Number => Value::Float(setting.default.parse().unwrap_or(0.0)),
        SettingTy::Choice(_) => Value::Str(setting.default.to_string()),
    }
}

/// Whether an action's subject is a setting rather than the story: the gate a caller applies before
/// [`write`].
///
/// One list, because a player and a test run that disagreed about which actions are the *player's* state
/// would write different worlds from the same press (`RUNTIME.md §2.1`) — and because the file page is
/// three more names in the same family, moved by the same [`write`].
#[must_use]
pub fn is_write(action: &Action) -> bool {
    matches!(
        action.name.as_str(),
        PREFERENCE
            | TOGGLE_PREFERENCE
            | crate::actions::FILE_PAGE
            | crate::actions::FILE_PAGE_NEXT
            | crate::actions::FILE_PAGE_PREVIOUS
    )
}

/// Writes what a `preference`, `toggle_preference`, or file-page action asks for, and answers the name it
/// wrote.
///
/// One place, because both callers carry the same actions out: the windowed player, where a settings
/// screen's button is the only way a player can change one, and the test runner, where a click is. What
/// they do with the answer differs — a player rewrites its settings file, a run says nothing — and what a
/// setting *means* is here. The file page rides the same path for the same reason: it is the player's
/// state, and Ren'Py keeps it in the same store (`persistent._file_page`).
///
/// `None` when the action is none of them; when it names a setting this build does not have (not a typo
/// the checker would have let through — `E5020` is where it is written — but a bundle built by a build
/// whose vocabulary was different); or when the value is one the world cannot hold, which is an action or
/// a record. Each of those is a caller's to report, and a caller that says so beats one that writes a
/// setting nothing reads.
///
/// A **toggle flips**, reading the declaration's default when nothing is stored yet — which is what lets
/// a checkbox flip a setting that no screen can read (`SCREENS.md §7.1`).
pub fn write(preferences: &mut Preferences, action: &Action) -> Option<String> {
    match action.name.as_str() {
        crate::actions::FILE_PAGE => {
            let page = match action.args.first() {
                Some(ScreenValue::Num(number)) if *number >= 1.0 => *number as u32,
                // A page a screen computed and got wrong is the first page, which is `page`'s rule for
                // anything that cannot be a position: the press still moves the screen somewhere it can
                // draw (`SCREENS.md §7`).
                _ => 1,
            };
            preferences.set(PAGE, Value::Int(i64::from(page)));
            Some(PAGE.to_string())
        }
        crate::actions::FILE_PAGE_NEXT => {
            let next = next_page(preferences);
            preferences.set(PAGE, Value::Int(i64::from(next)));
            Some(PAGE.to_string())
        }
        crate::actions::FILE_PAGE_PREVIOUS => {
            let previous = previous_page(preferences);
            preferences.set(PAGE, Value::Int(i64::from(previous)));
            Some(PAGE.to_string())
        }
        PREFERENCE => {
            let (name, value) = (action.first()?, action.args.get(1)?);
            SettingDecl::named(name)?;
            preferences.set(name, world_value(value)?);
            Some(name.to_string())
        }
        TOGGLE_PREFERENCE => {
            let name = action.first()?;
            let setting = SettingDecl::named(name)?;
            // Nothing stored yet is the declaration's default, so the first press of a checkbox is a
            // *change* rather than a no-op on a value the player never chose.
            let stored = match preferences.get(name) {
                Some(Value::Bool(stored)) => *stored,
                _ => setting.default == "true",
            };
            preferences.set(name, Value::Bool(!stored));
            Some(name.to_string())
        }
        _ => None,
    }
}

/// A screen's value as the world holds one.
///
/// The two types are different on purpose — a screen's value may be an *action*, and the world's may be a
/// struct — so this crossing is where what a setting can hold is decided: a string, a number, a boolean,
/// a list of those, or nothing at all. An action or a record is refused rather than flattened into
/// something that looks like it worked.
fn world_value(value: &ScreenValue) -> Option<Value> {
    match value {
        ScreenValue::Str(text) => Some(Value::Str(text.clone())),
        ScreenValue::Num(number) => Some(Value::Float(*number)),
        ScreenValue::Bool(flag) => Some(Value::Bool(*flag)),
        ScreenValue::List(items) => items
            .iter()
            .map(world_value)
            .collect::<Option<Vec<_>>>()
            .map(Value::List),
        ScreenValue::None => Some(Value::None),
        ScreenValue::Action(_) | ScreenValue::Record(_) => None,
    }
}
