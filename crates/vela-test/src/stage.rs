//! The screens a headless run can click.
//!
//! A test that clicks a control is a player minus the window (`TOOLING.md §5`), and this is what it
//! has instead of one: the project's compiled screens, the stack of the ones it has open, and a text
//! engine to size them with. Everything a laid screen needs and nothing that needs a display — the
//! same [`Stack`] `vela run` drives, which is why the two cannot disagree about what a control does.
//!
//! The frame is a constant here rather than the window's: a test is deterministic by construction, and
//! a click that landed on a different control at 1920×1080 than at 1280×720 would be a test about the
//! machine. So it is the reference frame — the design size a project that declares none is drawn for.

use vela_text::TextEngine;
use vela_ui::actions::Action;
use vela_ui::{Done, ScreenSet, Stack};
use vela_world::Preferences;

/// The frame a headless run lays screens out in.
///
/// A project's own design size is a runtime fact (`SCREENS.md §2.6`) and this is not it: a test is
/// laid out in the reference frame so that what a test sees is what the source says rather than what
/// the machine running it has.
pub const FRAME: (u32, u32) = (1280, 720);

/// The screens a run can see, and the ones it has open.
pub struct Stage<'a> {
    /// The project's screens, one set per file (`SCREENS.md §5`).
    screens: &'a [ScreenSet],
    /// The screens the story opened, bottom first.
    overlays: Stack,
    /// The engine a laid screen sizes its text with.
    text: &'a mut TextEngine,
    /// The face that engine knows.
    font: &'a str,
}

impl<'a> Stage<'a> {
    /// A stage over a project's screens, with nothing open.
    #[must_use]
    pub fn new(screens: &'a [ScreenSet], text: &'a mut TextEngine, font: &'a str) -> Self {
        Self {
            screens,
            overlays: Stack::default(),
            text,
            font,
        }
    }

    /// Opens a screen by name, and says whether the project declares one.
    ///
    /// Returns `false` rather than failing, because the caller is the one that knows what a missing
    /// screen means: a window says so and carries on, and a test reports it.
    pub fn open(&mut self, name: &str) -> bool {
        self.overlays
            .open(self.screens, name, &[], FRAME, self.text, self.font)
    }

    /// Whether nothing is open.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.overlays.is_empty()
    }

    /// Moves focus to the top screen's control whose words contain `text`, and returns what it asks
    /// for. `None` when nothing on the top screen reads those words — including when nothing is open.
    ///
    /// Focus rather than the action, so that a click and a keypress reach the same code and land the
    /// focus ring where the press did (`Stack::focus_label`).
    #[must_use]
    pub fn focus(&mut self, text: &str) -> Option<Action> {
        if !self.overlays.focus_label(text) {
            return None;
        }
        self.overlays.focused().cloned()
    }

    /// Every word the top screen's controls read, in focus order and without repeats.
    ///
    /// What a failure prints when a click names nothing: *what was there instead*. Two controls may
    /// draw the same words — a button inside a button — and a reader wants the offer once.
    #[must_use]
    pub fn controls(&self) -> Vec<String> {
        let Some(top) = self.overlays.top() else {
            return Vec::new();
        };
        let mut words: Vec<String> = Vec::new();
        for hotspot in &top.laid.hotspots {
            let label = hotspot.label.trim();
            if label.is_empty() || words.iter().any(|seen| seen == label) {
                continue;
            }
            words.push(label.to_string());
        }
        words
    }

    /// Carries out what a control asked for — the part of an action the screen stack owns.
    ///
    /// Everything else comes back as [`Done::NotOurs`]: a headless run has no VM behind it, and the
    /// caller says so rather than pretending the press did something.
    pub fn carry_out(&mut self, action: &Action) -> Done {
        self.overlays
            .dispatch(action, self.screens, FRAME, self.text, self.font)
    }

    /// Tells the stack what the player has chosen, so a screen can read a setting (`RUNTIME.md §2.1`).
    pub fn set_preferences(&mut self, preferences: Preferences) {
        self.overlays.set_preferences(preferences);
    }

    /// Lays every open screen out again, at the store the stack now holds.
    ///
    /// What a step that changed a setting calls: a checkbox draws its own new state, so the *next* click
    /// offers what the screen says now rather than what it said before the press (`SCREENS.md §7.1`).
    pub fn relaid(&mut self) {
        self.overlays
            .relaid(self.screens, FRAME, self.text, self.font);
    }
}
