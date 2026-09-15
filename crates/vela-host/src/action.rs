//! Semantic actions, and the bindings that produce them.
//!
//! `SCREENS.md §11`: input is abstracted to actions, never to keys. A screen asks "was I
//! advanced", not "was Enter pressed", because the answer has to be the same whether the
//! player used a keyboard, a gamepad, or a touch screen — and because a project that wants
//! `Space` instead of `Enter` should be a binding change rather than a code change.
//!
//! Bindings are a table here rather than a callback, so what a key does is inspectable: a
//! test can assert it, a tool can print it, and a project can override it without a closure.

/// What a device event means to a story.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Action {
    /// Continue past the current line.
    Advance,
    /// Hold to fast-forward.
    Skip,
    /// Go back.
    Rollback,
    /// Move the menu selection up.
    MenuUp,
    /// Move the menu selection down.
    MenuDown,
    /// Accept the current choice.
    Confirm,
    /// Dismiss, or open the menu.
    Cancel,
    /// Ask for a screenshot.
    Screenshot,
    /// Leave.
    Quit,
}

impl Action {
    /// The name a binding profile writes.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Advance => "advance",
            Self::Skip => "skip",
            Self::Rollback => "rollback",
            Self::MenuUp => "menu_up",
            Self::MenuDown => "menu_down",
            Self::Confirm => "confirm",
            Self::Cancel => "cancel",
            Self::Screenshot => "screenshot",
            Self::Quit => "quit",
        }
    }

    /// Every action, in a stable order.
    #[must_use]
    pub fn all() -> &'static [Self] {
        &[
            Self::Advance,
            Self::Skip,
            Self::Rollback,
            Self::MenuUp,
            Self::MenuDown,
            Self::Confirm,
            Self::Cancel,
            Self::Screenshot,
            Self::Quit,
        ]
    }

    /// The action a profile names.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::all().iter().copied().find(|a| a.as_str() == name)
    }
}

/// A named key, in the small vocabulary a binding profile needs.
///
/// Not `winit`'s key type: `vela-host` is the only crate that should know a windowing
/// library exists, and a profile written against a platform type could not be read by the
/// LSP or checked by the compiler.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Key {
    /// A character key, by its lowercase character.
    Char(char),
    /// Enter or Return.
    Enter,
    /// Space.
    Space,
    /// Escape.
    Escape,
    /// Shift.
    Shift,
    /// Left Control.
    Control,
    /// Arrow up.
    Up,
    /// Arrow down.
    Down,
    /// Arrow left.
    Left,
    /// Arrow right.
    Right,
    /// Tab.
    Tab,
    /// Backspace.
    Backspace,
    /// A mouse button, by number: 1 is primary.
    Mouse(u8),
}

impl Key {
    /// The name a profile writes.
    #[must_use]
    pub fn as_str(&self) -> String {
        match self {
            Self::Char(c) => c.to_string(),
            Self::Enter => "enter".to_string(),
            Self::Space => "space".to_string(),
            Self::Escape => "escape".to_string(),
            Self::Shift => "shift".to_string(),
            Self::Control => "control".to_string(),
            Self::Up => "up".to_string(),
            Self::Down => "down".to_string(),
            Self::Left => "left".to_string(),
            Self::Right => "right".to_string(),
            Self::Tab => "tab".to_string(),
            Self::Backspace => "backspace".to_string(),
            Self::Mouse(button) => format!("mouse{button}"),
        }
    }

    /// The key a profile names.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "enter" | "return" => Some(Self::Enter),
            "space" => Some(Self::Space),
            "escape" | "esc" => Some(Self::Escape),
            "shift" => Some(Self::Shift),
            "control" | "ctrl" => Some(Self::Control),
            "up" => Some(Self::Up),
            "down" => Some(Self::Down),
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "tab" => Some(Self::Tab),
            "backspace" => Some(Self::Backspace),
            _ => name
                .strip_prefix("mouse")
                .and_then(|number| number.parse().ok())
                .map(Self::Mouse)
                .or_else(|| {
                    let mut characters = name.chars();
                    match (characters.next(), characters.next()) {
                        (Some(c), None) => Some(Self::Char(c.to_ascii_lowercase())),
                        _ => None,
                    }
                }),
        }
    }
}

/// What each key does.
///
/// Ordered by `Key` rather than hashed, because the project forbids `HashMap` for exactly the
/// reason it matters here: a profile is data, and data that iterates in an unpredictable
/// order cannot be printed, diffed, or golden-tested.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Bindings {
    entries: Vec<(Key, Action)>,
}

impl Default for Bindings {
    /// The defaults: what a visual novel player already has in their fingers.
    fn default() -> Self {
        Self::new()
    }
}

impl Bindings {
    /// The built-in profile.
    #[must_use]
    pub fn new() -> Self {
        let entries = vec![
            (Key::Enter, Action::Advance),
            (Key::Space, Action::Advance),
            (Key::Mouse(1), Action::Advance),
            (Key::Control, Action::Skip),
            (Key::Tab, Action::Skip),
            (Key::Backspace, Action::Rollback),
            // Arrows rather than `w`/`s`: a letter that moves a menu is also a letter the
            // player will press for something else, and one key cannot mean two things here.
            (Key::Up, Action::MenuUp),
            (Key::Down, Action::MenuDown),
            (Key::Escape, Action::Cancel),
            (Key::Char('p'), Action::Screenshot),
            (Key::Char('q'), Action::Quit),
        ];
        Self { entries }
    }

    /// An empty profile.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// The bindings a named profile installs, if it is one.
    ///
    /// Targets carry a profile (`BUILD_AND_ASSETS.md §4`), and a bundle names its own, so this is
    /// how a build's declared input defaults actually reach the window rather than being a field
    /// nothing reads. `None` for an unknown name is deliberate: a bundle that asked for a profile
    /// this build does not have should be told, not quietly given the default.
    #[must_use]
    pub fn profile(name: &str) -> Option<Self> {
        match name {
            "keyboard-mouse" => Some(Self::new()),
            "keyboard" => {
                let mut bindings = Self::new();
                bindings.unbind(Key::Mouse(1));
                Some(bindings)
            }
            "pointer" => {
                let mut bindings = Self::empty();
                bindings.bind(Key::Mouse(1), Action::Advance);
                bindings.bind(Key::Escape, Action::Cancel);
                Some(bindings)
            }
            _ => None,
        }
    }

    /// Binds a key, replacing any previous binding.
    pub fn bind(&mut self, key: Key, action: Action) {
        self.entries.retain(|(bound, _)| *bound != key);
        self.entries.push((key, action));
        self.entries.sort();
    }

    /// Unbinds a key.
    pub fn unbind(&mut self, key: Key) {
        self.entries.retain(|(bound, _)| *bound != key);
    }

    /// What a key does.
    #[must_use]
    pub fn action(&self, key: Key) -> Option<Action> {
        self.entries
            .iter()
            .find(|(bound, _)| *bound == key)
            .map(|(_, action)| *action)
    }

    /// The bindings, in key order.
    #[must_use]
    pub fn entries(&self) -> &[(Key, Action)] {
        &self.entries
    }
}
