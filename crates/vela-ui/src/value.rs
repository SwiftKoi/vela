//! The values a screen body holds, and the scope it holds them in.
//!
//! Split from `eval.rs` by `REPO_LAYOUT.md §3.1`'s third recipe — the data model out of the
//! algorithms that read it. A `Value` is what a screen was handed or built (`SCREENS.md §7`, `§2.4`),
//! and an `Args` is the scope those values live in; the evaluator beside this file is what produces
//! one from an expression.

use vela_world::Preferences;

use crate::actions::Action;
use crate::slots::Slot;
use crate::variants::Variants;

/// A value a screen argument can hold.
///
/// The small set a screen body can branch on. A `screen` is called by the runtime with
/// arguments it already has as `World` values, and this is the shape they arrive in.
#[derive(Clone, PartialEq, Debug)]
pub enum Value {
    /// A string.
    Str(String),
    /// A number.
    Num(f64),
    /// A boolean.
    Bool(bool),
    /// An action: what a widget does when it is activated (`SCREENS.md §7`).
    ///
    /// An action is a *value* and not only a syntax, which is what lets a screen take one as a
    /// parameter and hand it to a widget. Without that, `confirm(message, yes_action, no_action)`
    /// cannot be written at all: the caller's answer is the content.
    Action(Action),
    /// A sequence, for a `for` to walk (`SCREENS.md §2.4`).
    ///
    /// A list rather than anything cleverer: a screen draws children from data, and the data arrives
    /// from outside it. Nothing *builds* one in a screen body except a literal, which is what makes a
    /// loop testable before the systems that feed it exist.
    List(Vec<Value>),
    /// A record: named fields, so `option.caption` resolves (`SCREENS.md §2.4`).
    ///
    /// Ordered rather than hashed, like everything else that reaches output — two runs of one screen
    /// must walk the same fields in the same order.
    Record(Vec<(String, Value)>),
    /// No value.
    None,
}

impl Value {
    /// The value as text, for `text <name>`.
    #[must_use]
    pub fn as_text(&self) -> String {
        match self {
            Self::Str(text) => text.clone(),
            Self::Bool(true) => "true".to_string(),
            Self::Bool(false) => "false".to_string(),
            // An action as text is the call it was written as. Nothing draws one today, but a screen
            // that interpolates an action should say what it is rather than render as blank.
            Self::Action(action) => action.to_string(),
            // A sequence and a record render as themselves, `[a, b]` and `{caption: "Yes"}`. Nothing
            // draws one on purpose, and the alternative — blank — is a screen that looks like it
            // worked. Elements go through this same function, so a nested value cannot print
            // differently depending on where it sits.
            Self::List(items) => {
                let parts: Vec<String> = items.iter().map(Self::as_text).collect();
                format!("[{}]", parts.join(", "))
            }
            Self::Record(fields) => {
                let parts: Vec<String> = fields
                    .iter()
                    .map(|(name, value)| format!("{name}: {}", value.as_text()))
                    .collect();
                format!("{{{}}}", parts.join(", "))
            }
            Self::None => String::new(),
            // Not `{}`: a float's default formatting is locale-adjacent enough that the
            // determinism rules ban it, and a fixed precision is what a screen wants anyway.
            Self::Num(number) if number.fract() == 0.0 => format!("{number:.0}"),
            Self::Num(number) => format!("{number:.2}"),
        }
    }

    /// Whether the value counts as true in a screen condition.
    #[must_use]
    pub fn truthy(&self) -> bool {
        match self {
            Self::None => false,
            Self::Bool(value) => *value,
            Self::Str(text) => !text.is_empty(),
            Self::Num(number) => *number != 0.0,
            // An action is something, so it is true — the same answer every non-`none` value gets.
            Self::Action(_) => true,
            // Empty is false, which is Python's answer and Ren'Py's: `if items:` is how a screen asks
            // whether it was handed anything to draw.
            Self::List(items) => !items.is_empty(),
            Self::Record(fields) => !fields.is_empty(),
        }
    }

    /// The fields of a record, if this value is one.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&Value> {
        let Self::Record(fields) = self else {
            return None;
        };
        fields
            .iter()
            .find(|(field, _)| field == name)
            .map(|(_, value)| value)
    }
}

/// The arguments a screen was called with, by parameter name, in call order.
///
/// Also carries what the host says about *where* the screen is running ([`Variants`]) and what the player
/// has chosen ([`Preferences`]), because those are the two things a screen may read besides its own names
/// (`SCREENS.md §2.6`, `RUNTIME.md §2.1`): both have to reach an arm, a loop body and a `use` argument
/// exactly the way a name does, and a scope that carried one and not the other would make `variant(...)`
/// or `setting(...)` depend on which walk was asking.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Args {
    values: Vec<(String, Value)>,
    variants: Variants,
    preferences: Preferences,
    slots: Vec<Slot>,
}

impl Args {
    /// No arguments.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// What the host says about where this screen is running.
    #[must_use]
    pub fn variants(&self) -> Variants {
        self.variants
    }

    /// Sets the variants this scope decides with.
    pub fn set_variants(&mut self, variants: Variants) {
        self.variants = variants;
    }

    /// This scope, with the variants the host reports.
    #[must_use]
    pub fn with_variants(mut self, variants: Variants) -> Self {
        self.set_variants(variants);
        self
    }

    /// What the player has chosen (`RUNTIME.md §2.1`).
    #[must_use]
    pub fn preferences(&self) -> &Preferences {
        &self.preferences
    }

    /// Sets the player's settings this scope answers with.
    pub fn set_preferences(&mut self, preferences: Preferences) {
        self.preferences = preferences;
    }

    /// This scope, with the player's settings.
    #[must_use]
    pub fn with_preferences(mut self, preferences: Preferences) -> Self {
        self.set_preferences(preferences);
        self
    }

    /// The slots the host found (`slots::page`).
    ///
    /// The third thing a scope carries besides its names, for the same reason the other two are here: a
    /// save screen asks about slots from an arm, a loop body and a `use` argument, and an answer that
    /// depended on which walk was asking would be an answer about the walk rather than about the store.
    #[must_use]
    pub fn slots(&self) -> &[Slot] {
        &self.slots
    }

    /// Sets the slots this scope answers with.
    pub fn set_slots(&mut self, slots: Vec<Slot>) {
        self.slots = slots;
    }

    /// This scope, with the slots the host found.
    #[must_use]
    pub fn with_slots(mut self, slots: Vec<Slot>) -> Self {
        self.set_slots(slots);
        self
    }

    /// Binds a name.
    ///
    /// A name already bound keeps the *earlier* value when read, because [`get`](Self::get) answers with
    /// the first binding it finds: `set` adds a layer to a scope, and [`with`](Self::with) is what
    /// replaces one.
    pub fn set(&mut self, name: impl Into<String>, value: Value) {
        self.values.push((name.into(), value));
    }

    /// The value bound to a parameter, if any.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.values
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
    }

    /// This scope, with `name` bound to `value`.
    ///
    /// A clone with one binding replaced, which is what a loop needs: `for option in items` must
    /// *shadow* an outer `option` rather than be shadowed by it, and the shadow has to end with the
    /// iteration. [`get`](Self::get) answers with the first binding it finds, so a loop that appended
    /// would never be the one read.
    #[must_use]
    pub fn with(&self, name: &str, value: Value) -> Self {
        let mut values: Vec<(String, Value)> = self
            .values
            .iter()
            .filter(|(key, _)| key != name)
            .cloned()
            .collect();
        values.push((name.to_string(), value));
        Self {
            values,
            variants: self.variants,
            // A loop's shadow is about the *element*, and what the host says does not change with it
            // (`SCREENS.md §2.4`): the variants and the player's settings travel unchanged — and so do the
            // slots, which are a save screen's *data* rather than its arguments.
            preferences: self.preferences.clone(),
            slots: self.slots.clone(),
        }
    }
}

/// One screen instance's own variables (`SCREENS.md §2.5`).
///
/// What a `default` declares, and what `set_screen_variable` writes. Per *instance* rather than per
/// declaration: the runtime keeps one beside each screen it has open, so closing a screen and opening it
/// again starts from the initializers, and a hot reload of the screen's source leaves the values alone.
///
/// A distinct type over the shape [`Args`] has, because [`ScreenSet::lay`](crate::ScreenSet::lay) takes
/// one after the other and they are both `name → value`: a caller that swapped them would pass a
/// screen's state as its arguments, which type-checks and draws the wrong thing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScreenState {
    bound: Args,
}

impl ScreenState {
    /// A screen that has not been laid out yet: no variables.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The value bound to a variable, if the screen has one.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.bound.get(name)
    }

    /// Binds a variable.
    ///
    /// Replaces rather than shadows, which is the difference between this and a scope: a store has one
    /// value per name, and a write is the operation that changes it.
    pub fn set(&mut self, name: &str, value: Value) {
        self.bound = self.bound.with(name, value);
    }

    /// Every variable, in the order the screen declares them.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Value)> {
        self.bound.values.iter().map(|(name, value)| (name, value))
    }

    /// How many variables it holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bound.values.len()
    }

    /// Whether it holds none — a screen with no `default`, which is most of them.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bound.values.is_empty()
    }
}
