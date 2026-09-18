//! Reading what `gui.rpy` says: one variable at a time, and the values they hold.

use crate::rpy::{Kind, Node};

/// One `define gui.<name> = <value>`.
pub(super) struct Variable {
    /// The line it came from, 1-based.
    pub(super) line: u32,
    /// The name without its `gui.` prefix.
    pub(super) name: String,
    /// The right-hand side, classified.
    pub(super) value: Value,
}

/// What a `gui.rpy` value can be.
#[derive(Clone, PartialEq, Debug)]
pub(super) enum Value {
    /// A colour: its `0xRRGGBB`, and its alpha (255 when the source wrote none).
    ///
    /// Ren'Py writes colours as `'#cc6600'` and sometimes as `'#5555557f'`. A Vela theme colour is
    /// opaque — `0xRRGGBB`, and the painter refuses a longer value — so the alpha is carried here
    /// to be *reported* rather than dropped in silence.
    Colour {
        /// The red, green and blue, as `0xRRGGBB`.
        rgb: u32,
        /// The alpha, `255` when the source wrote none.
        alpha: u8,
    },
    /// A number, as written.
    Number(String),
    /// A font file, as the face name a project would register it under (`DejaVuSans.ttf` →
    /// `DejaVuSans`).
    Face(String),
    /// A quoted string that is neither a colour, nor a picture, nor a font: `"hide"`, `"unicode"`.
    Text(String),
    /// Something this pass does not read: a call, an expression, `None`.
    Other,
}

/// Every `define gui.<name> = <value>` in the file, in source order.
pub(super) fn variables(nodes: &[Node]) -> Vec<Variable> {
    let mut found = Vec::new();
    for node in nodes {
        if let Kind::Define { name, value } = &node.kind {
            if let Some(name) = name.strip_prefix("gui.") {
                found.push(Variable {
                    line: node.line,
                    name: name.to_string(),
                    value: Value::parse(value),
                });
            }
        }
        found.extend(variables(&node.children));
    }
    found
}

impl Value {
    /// Classifies a right-hand side as written.
    fn parse(text: &str) -> Value {
        let text = text.trim();
        if let Some(colour) = text.strip_prefix("0x").and_then(integer_colour) {
            return colour;
        }
        if let Some(colour) = quoted(text).and_then(hex_colour) {
            return colour;
        }
        if text.parse::<f64>().is_ok() {
            return Value::Number(text.to_string());
        }
        match quoted(text) {
            // A font is a *file*, which is what tells one from `"hide"` and `"unicode"` — two of
            // gui.rpy's strings that are settings, not faces, and that a name-based rule alone
            // would have emitted as `font unscrollable = "hide"`.
            Some(text) if is_font(text) => Value::Face(face_of(text)),
            Some(text) => Value::Text(text.to_string()),
            None => Value::Other,
        }
    }
}

/// The contents of a single-quoted or double-quoted string.
fn quoted(text: &str) -> Option<&str> {
    text.strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            text.strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
        })
}

/// `#rrggbb` or `#rrggbbaa` as a colour, if that is what it is.
fn hex_colour(text: &str) -> Option<Value> {
    let digits = text.strip_prefix('#')?;
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) || !matches!(digits.len(), 6 | 8) {
        return None;
    }
    let value = u32::from_str_radix(digits, 16).ok()?;
    let rgb = if digits.len() == 8 { value >> 8 } else { value };
    Some(Value::Colour {
        rgb,
        alpha: if digits.len() == 8 {
            (value & 0xFF) as u8
        } else {
            255
        },
    })
}

/// `0xRRGGBB` or `0xRRGGBBAA` as a colour, if that is what it is.
fn integer_colour(digits: &str) -> Option<Value> {
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(digits, 16).ok()?;
    match digits.len() {
        6 => Some(Value::Colour {
            rgb: value,
            alpha: 255,
        }),
        8 => Some(Value::Colour {
            rgb: value >> 8,
            alpha: (value & 0xFF) as u8,
        }),
        _ => None,
    }
}

/// Whether a string names a picture rather than a font: a path, by Vela's own rule for assets.
pub(super) fn is_picture(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [".png", ".jpg", ".jpeg", ".webp", ".svg", ".gif"]
        .iter()
        .any(|extension| lower.ends_with(extension))
}

/// Whether a string names a font file.
fn is_font(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [".ttf", ".otf", ".ttc", ".woff", ".woff2"]
        .iter()
        .any(|extension| lower.ends_with(extension))
}

/// A font face, as a project would register it: the file's name without its extension.
fn face_of(text: &str) -> String {
    let name = text.rsplit('/').next().unwrap_or(text);
    match name.rsplit_once('.') {
        Some((stem, _)) => stem.to_string(),
        None => name.to_string(),
    }
}

/// The colour of every colour variable, by variable name.
pub(super) fn colours(variables: &[Variable]) -> std::collections::BTreeMap<String, u32> {
    variables
        .iter()
        .filter_map(|variable| match variable.value {
            Value::Colour { rgb, .. } => Some((variable.name.clone(), rgb)),
            _ => None,
        })
        .collect()
}

/// The face of every font variable, by variable name.
pub(super) fn fonts(variables: &[Variable]) -> std::collections::BTreeMap<String, String> {
    variables
        .iter()
        .filter_map(|variable| match &variable.value {
            Value::Face(face) => Some((variable.name.clone(), face.clone())),
            _ => None,
        })
        .collect()
}

/// The frame `gui.init(width, height)` declares, if the file declares one.
pub(super) fn design(nodes: &[Node]) -> Option<(u32, u32)> {
    for node in nodes {
        if let Some(at) = node.text.find("gui.init(") {
            let inside = node.text[at + "gui.init(".len()..]
                .split(')')
                .next()
                .unwrap_or("");
            let numbers: Vec<u32> = inside
                .split(',')
                .filter_map(|part| part.trim().parse::<u32>().ok())
                .collect();
            if let [width, height] = numbers[..] {
                return Some((width, height));
            }
        }
        if let Some(found) = design(&node.children) {
            return Some(found);
        }
    }
    None
}
