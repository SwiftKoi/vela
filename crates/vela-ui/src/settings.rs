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
//! those by name rather than writing a button that does nothing.
//!
//! **The `read` flag is the honest half of a vocabulary that runs ahead of its systems.** Nothing
//! reads a setting yet: the screens that draw one and the transport that obeys one are this
//! milestone's item 4. Saying so here is the same shape `ActionDecl::dispatched` has, and it is what
//! stops "Vela has a text speed" from reading as "Vela types your dialogue out".

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
];

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
