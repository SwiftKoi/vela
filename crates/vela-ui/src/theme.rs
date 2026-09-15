//! Theme tokens, and the contrast lint.
//!
//! `SCREENS.md §5`: *"contrast checking: a foreground/background pair below a WCAG threshold
//! is `W4009`, with the computed ratio in the message. Accessibility as a lint, not a manual
//! audit."*
//!
//! The second sentence is the reason this exists rather than a checklist. A manual audit is a
//! document that is correct on the day it is written and drifts with every theme edit; a lint
//! re-runs on every build, and it says *the ratio is 2.8:1* rather than *this looks wrong*,
//! because a number is something an author can act on.
//!
//! The convention this checks against: a theme's `bg` token is its background, and every other
//! colour is a foreground on it. That is what the spec's own example implies — `bg`, `fg`,
//! `accent` — and it is the only pairing a theme can express without a screen to say which
//! surface a colour lands on. Screens that pair colours themselves are checked when they
//! resolve, not here.

use vela_diag::{Code, Diagnostic};
use vela_render::Color;
use vela_span::Span;
use vela_syntax::{Expr, ThemeDecl};

/// The token that names a theme's background.
///
/// A convention rather than a keyword, and stated as one: `SCREENS.md §5`'s example names it
/// `bg`, and a lint needs *some* agreed background to measure against.
pub const BACKGROUND_TOKEN: &str = "bg";

/// A colour, as sRGB bytes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rgb {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
}

impl Rgb {
    /// This colour, in the renderer's colour type. The one conversion between the two.
    #[must_use]
    pub fn color(self) -> Color {
        Color::rgb(self.r, self.g, self.b)
    }

    /// The relative luminance of this colour, per WCAG 2.1.
    ///
    /// The sRGB channels are linearised first, and the weights are the standard's own: the eye
    /// is far more sensitive to green than to blue, which is why a "50% grey" background and a
    /// saturated blue of the same nominal brightness look nothing alike.
    #[must_use]
    pub fn luminance(&self) -> f64 {
        fn channel(value: u8) -> f64 {
            let value = f64::from(value) / 255.0;
            if value <= 0.039_28 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * channel(self.r) + 0.7152 * channel(self.g) + 0.0722 * channel(self.b)
    }

    /// The contrast ratio against another colour, from 1.0 to 21.0.
    #[must_use]
    pub fn contrast(&self, other: &Self) -> f64 {
        let (a, b) = (self.luminance(), other.luminance());
        let (lighter, darker) = if a > b { (a, b) } else { (b, a) };
        (lighter + 0.05) / (darker + 0.05)
    }
}

/// The minimum ratio for body text, from WCAG 2.1 level AA.
pub const AA_TEXT: f64 = 4.5;

/// The colours a theme declares.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Palette {
    /// The colours, by token name, in declaration order.
    pub colors: Vec<(String, Rgb)>,
}

impl Palette {
    /// A colour by token name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<Rgb> {
        self.colors
            .iter()
            .find(|(token, _)| token == name)
            .map(|(_, rgb)| *rgb)
    }

    /// Whether the theme declares no colours at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.colors.is_empty()
    }

    /// A token's colour, in the renderer's type.
    #[must_use]
    pub fn color(&self, token: &str) -> Option<Color> {
        self.get(token).map(Rgb::color)
    }
}

/// Reads a theme's colour tokens.
///
/// Only colours: a theme also carries spacing and fonts, and a palette that pretended to hold
/// them would be a palette with three meanings.
#[must_use]
pub fn palette(theme: &ThemeDecl) -> Palette {
    let mut colors = Vec::new();
    for setting in &theme.settings {
        let Expr::Int { value, .. } = setting.value else {
            continue;
        };
        // A colour is what the theme says is one. `color bg = 0x10121a` carries the type
        // word, and it is the only thing that can tell `bg` from `sm` in `space sm = 4`:
        // both arrive as an integer, and the parser does not record how either was written.
        // An earlier version of this guessed from the magnitude of the number, which worked
        // and was the wrong shape — it made `sm = 4` a near-black colour and would have made
        // `space lg = 0x100000` a colour too.
        if setting.ty.as_deref() != Some("color") {
            continue;
        }
        let Ok(value) = u32::try_from(value) else {
            continue;
        };
        if value > 0xFF_FFFF {
            continue;
        }
        colors.push((
            setting.key.clone(),
            Rgb {
                r: ((value >> 16) & 0xFF) as u8,
                g: ((value >> 8) & 0xFF) as u8,
                b: (value & 0xFF) as u8,
            },
        ));
    }
    Palette { colors }
}

/// Checks every foreground token against the background.
///
/// Returns nothing for a theme with no `bg`, because there is no pair to measure and inventing
/// a default background would measure a colour the author never chose.
#[must_use]
pub fn check_contrast(theme: &ThemeDecl) -> Vec<Diagnostic> {
    let palette = palette(theme);
    let Some(background) = palette.get(BACKGROUND_TOKEN) else {
        return Vec::new();
    };

    let mut diagnostics = Vec::new();
    for (token, colour) in &palette.colors {
        if token == BACKGROUND_TOKEN {
            continue;
        }
        let ratio = colour.contrast(&background);
        if ratio >= AA_TEXT {
            continue;
        }
        diagnostics.push(
            diag(
                "W4009",
                format!(
                    "`{token}` on `{BACKGROUND_TOKEN}` has a contrast ratio of {ratio:.2}:1, \
                     below the {AA_TEXT:.1}:1 minimum"
                ),
                theme.span,
                "this pair is hard to read",
            )
            .with_help("lighten the foreground, darken the background, or raise this pair")
            .with_note(format!(
                "the foreground is #{:02x}{:02x}{:02x} and the background #{:02x}{:02x}{:02x}",
                colour.r, colour.g, colour.b, background.r, background.g, background.b
            )),
        );
    }
    diagnostics
}

/// Builds a diagnostic for a registered code.
///
/// # Panics
///
/// Panics if the code is not registered, which is a bug here rather than anything a user can
/// trigger: `check-diag-codes` rejects an unregistered code before it can be committed.
fn diag(
    code: &str,
    message: impl Into<String>,
    span: Span,
    label: impl Into<String>,
) -> Diagnostic {
    let code =
        Code::new(code).unwrap_or_else(|| panic!("`{code}` is not in crates/vela-diag/codes.txt"));
    Diagnostic::new(code, message, span, label)
}
