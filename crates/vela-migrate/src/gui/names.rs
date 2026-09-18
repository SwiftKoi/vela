//! The naming convention: which style a `gui.` variable belongs to, and what Vela can do with it.
//!
//! This is where the translation actually happens, and it is one question asked twice. *Is this
//! variable's group a style a screen's splat names?* — if it is, the variable is a property of that
//! style, and the key is what is left of the name once the group and Ren'Py's `text_` infix are
//! gone. If it is not, the variable is a palette entry, a font token, or a value with nowhere to go.

use std::collections::{BTreeMap, BTreeSet};

use super::values::{Value, Variable, is_picture};

/// The properties a Vela style **paints**, and therefore the only ones worth emitting.
///
/// Measured, not assumed: `crates/vela-ui/src/paint.rs` resolves exactly these when it applies a
/// style, and a probe through `vela run --capture` confirms that `size = 40` in a style draws
/// bigger text while `xpos = 400` and `xalign = 1.0` are accepted and ignored.
const PAINTED: &[&str] = &["color", "background", "size", "font"];

/// The interaction states a key may be prefixed with (`SCREENS.md §5.1`).
const STATES: &[&str] = &["idle", "hover", "selected", "insensitive"];

/// What one `gui.` variable becomes.
pub(super) enum Fate {
    /// A style setting: the style, the key, and the value as Vela writes it.
    Setting(String, String, String),
    /// A palette or font token: in the theme either way, and worth a report only when a migrated
    /// screen does not read it, because then nothing in the project uses it.
    Palette {
        /// Whether a screen reads it.
        read: bool,
    },
    /// A picture a screen places.
    Picture,
    /// A property a Vela style does not carry: the group, and the key.
    Placement(String, String),
    /// A name with no style group at all.
    Loose,
}

/// Decides what one variable becomes.
pub(super) fn classify(
    variable: &Variable,
    groups: &BTreeSet<String>,
    live: &BTreeSet<String>,
    colour_of: &BTreeMap<String, u32>,
    font_of: &BTreeMap<String, String>,
) -> Fate {
    let (group, key) = split(&variable.name, groups);
    if !groups.contains(&group) {
        if group == variable.name {
            // `gui.language = "unicode"`: there is no style it could belong to, and filing it under
            // a style named after itself would invent one.
            return Fate::Loose;
        }
        return match variable.value {
            Value::Colour { .. } | Value::Face(_) => Fate::Palette {
                read: live.contains(&variable.name),
            },
            _ => Fate::Placement(group, key),
        };
    }
    if let Value::Text(text) = &variable.value {
        if is_picture(text) {
            return Fate::Picture;
        }
    }
    match painted(&variable.value, colour_of, font_of, &key) {
        Some(setting) => Fate::Setting(group, key, setting),
        None if variable.value == Value::Other => Fate::Palette { read: true },
        None => Fate::Placement(group, key),
    }
}

/// One style setting, or `None` when Vela has no place for the property.
fn painted(
    value: &Value,
    colour_of: &BTreeMap<String, u32>,
    font_of: &BTreeMap<String, String>,
    key: &str,
) -> Option<String> {
    if !PAINTED.contains(&strip_state(key)) {
        return None;
    }
    match value {
        Value::Colour { rgb, .. } => Some(match unique(colour_of, *rgb) {
            Some(name) => format!("theme.{}", token(&name, "_color")),
            None => format!("0x{rgb:06x}"),
        }),
        Value::Face(face) => Some(match unique(font_of, face.clone()) {
            Some(name) => format!("theme.{}", token_for_font(&name)),
            None => format!("\"{face}\""),
        }),
        Value::Number(number) => Some(number.clone()),
        _ => None,
    }
}

/// The name of the *only* entry whose value matches, if there is exactly one.
///
/// Linking a style to a theme token is a nicety — it is what makes a style read the palette instead
/// of repeating a number — and doing it by value only works when the value means one thing.
/// `#ffffff` is four different roles in the sample's own `gui.rpy`, and resolving an `interface`
/// style's colour to `theme.choice_button_text_hover` because it sorts first would be a confident
/// wrong answer. Ambiguity keeps the literal, which is never wrong.
fn unique<T: PartialEq>(map: &BTreeMap<String, T>, wanted: T) -> Option<String> {
    let mut holding = map.iter().filter(|(_, value)| **value == wanted);
    let (name, _) = holding.next()?;
    if holding.next().is_some() {
        return None;
    }
    Some(name.clone())
}

/// Splits a gui variable name into the style it belongs to and the key it sets.
///
/// The group comes from the *screens*: `properties gui.text_properties("name")` is what makes
/// `name` a style, and the longest group a name starts with wins, so `button_text_hover_color` is
/// `button`'s `hover_color` rather than `button_text`'s `text_hover_color`. A name no splat covers
/// falls back to its own shape — a known state infix, or the last underscore.
pub(super) fn split(name: &str, groups: &BTreeSet<String>) -> (String, String) {
    if let Some(group) = groups
        .iter()
        .filter(|group| name.starts_with(&format!("{group}_")))
        .max_by_key(|group| group.len())
    {
        let rest = name[group.len() + 1..].to_string();
        return (group.clone(), strip_text_prefix(rest));
    }
    match STATES
        .iter()
        .find(|state| name.contains(&format!("_{state}_")))
    {
        Some(state) => {
            let (group, rest) = name.split_once(&format!("_{state}_")).unwrap_or((name, ""));
            (group.to_string(), format!("{state}_{rest}"))
        }
        None => match name.rsplit_once('_') {
            Some((group, prop)) => (group.to_string(), prop.to_string()),
            None => (name.to_string(), name.to_string()),
        },
    }
}

/// `text_font` is the `font` of the style's text, which is the style's own font in Vela.
fn strip_text_prefix(key: String) -> String {
    key.strip_prefix("text_").unwrap_or(&key).to_string()
}

/// The key without its state prefix, which is the property Vela resolves.
fn strip_state(key: &str) -> &str {
    match key.split_once('_') {
        Some((state, prop)) if STATES.contains(&state) => prop,
        _ => key,
    }
}

/// A theme token name: the variable's name with its type word removed.
pub(super) fn token(name: &str, suffix: &str) -> String {
    name.strip_suffix(suffix).unwrap_or(name).to_string()
}

/// The theme token for a font variable: the style it belongs to, so `name_text_font` is `name`.
pub(super) fn token_for_font(name: &str) -> String {
    name.strip_suffix("_text_font")
        .or_else(|| name.strip_suffix("_text"))
        .or_else(|| name.strip_suffix("_font"))
        .unwrap_or(name)
        .to_string()
}

/// The theme token for a font variable, resolving its group (`SCREENS.md §5`).
pub(super) fn font_token(name: &str, groups: &BTreeSet<String>) -> String {
    let (group, _) = split(name, groups);
    if group == *name {
        token_for_font(name)
    } else {
        // `button_text_font` is the `button` style's font, and the splat is what says so.
        token_for_font(&group)
    }
}

/// The variable names a screen uses: read directly, or covered by a properties splat.
pub(super) fn live_names(
    screens: &str,
    groups: &BTreeSet<String>,
    variables: &[Variable],
) -> BTreeSet<String> {
    let mut live = reads(screens);
    for group in groups {
        for variable in variables {
            if variable.name.starts_with(&format!("{group}_")) {
                live.insert(variable.name.clone());
            }
        }
    }
    live
}

/// Every `gui.<name>` a screen source reads.
///
/// A scan rather than a parse: `screens.rpy` is Ren'Py, and what is wanted here is which names
/// appear, which the text says as well as a parse would — and a name followed by `(` is a call
/// (`gui.text_properties`), not a variable.
fn reads(screens: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut index = 0;
    while let Some(at) = screens[index..].find("gui.") {
        let start = index + at + 4;
        let end = start
            + screens[start..]
                .bytes()
                .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
                .count();
        let name = &screens[start..end];
        if !name.is_empty() && !matches!(screens.as_bytes().get(end), Some(b'(')) {
            found.insert(name.to_string());
        }
        index = end.max(index + 1);
    }
    found
}

/// The groups `gui.<x>_properties("group")` names.
pub(super) fn splat_groups(screens: &str) -> BTreeSet<String> {
    let mut groups = BTreeSet::new();
    let mut rest = screens;
    while let Some(at) = rest.find("_properties(") {
        let after = &rest[at + "_properties(".len()..];
        if let Some(quoted) = after.trim_start().strip_prefix('"') {
            if let Some(end) = quoted.find('"') {
                groups.insert(quoted[..end].to_string());
            }
        }
        rest = &rest[at + "_properties(".len()..];
    }
    groups
}
