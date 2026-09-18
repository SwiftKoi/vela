//! Reading `vela.toml`.

use std::fmt;
use std::fs;
use std::path::Path;

use serde::Deserialize;

/// The manifest schema this build understands.
pub const SCHEMA: u32 = 1;

/// A project manifest.
#[derive(Debug, Deserialize)]
pub struct Manifest {
    /// The schema version, so a future format can be rejected rather than misread.
    pub schema: u32,
    /// Project settings.
    pub project: Project,
}

/// The frame a game is designed for (`SCREENS.md §2.6`).
///
/// Ren'Py calls this `gui.init(width, height)`, and it is what every position in a screen was
/// chosen against. Vela had no home for it until M12.1's variants made the engine need one: a
/// screen asking `variant("small")` is asking whether the frame it is being drawn in is smaller
/// than the one this game was designed for, which is a project fact and not an engine constant.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Frame {
    /// The design width, in logical pixels.
    pub width: u32,
    /// The design height, in logical pixels.
    pub height: u32,
}

impl Frame {
    /// What a project that does not declare one is designed for.
    ///
    /// Vela's own reference, and the frame its styles and its `--capture` default are written
    /// against. It is a default rather than a rule: a project that declares a size gets that one.
    pub const DEFAULT: Frame = Frame {
        width: vela_ui::REFERENCE_FRAME.0 as u32,
        height: vela_ui::REFERENCE_FRAME.1 as u32,
    };
}

impl Default for Frame {
    fn default() -> Self {
        Frame::DEFAULT
    }
}

impl fmt::Display for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

impl TryFrom<String> for Frame {
    type Error = String;

    /// Parses `"1280x720"`, and refuses anything else.
    ///
    /// A bad size is an error rather than a silent default, because the two are indistinguishable
    /// once a screen has drawn itself at the wrong size: `size = "1280,720"` or `size = "1280"` is
    /// a typo, and a typo that quietly changes what `small` means is one nobody would find.
    fn try_from(text: String) -> Result<Self, Self::Error> {
        let parsed = text.split_once(['x', 'X']).and_then(|(w, h)| {
            Some((w.trim().parse::<u32>().ok()?, h.trim().parse::<u32>().ok()?))
        });
        match parsed {
            Some((width, height)) if width > 0 && height > 0 => Ok(Frame { width, height }),
            _ => Err(format!(
                "`{text}` is not a design size: write it as `\"1280x720\"`"
            )),
        }
    }
}

impl<'de> Deserialize<'de> for Frame {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let text = String::deserialize(deserializer)?;
        Frame::try_from(text).map_err(serde::de::Error::custom)
    }
}

/// The `[project]` table.
#[derive(Debug, Deserialize)]
pub struct Project {
    /// The project's name, for a window title and for a bundle.
    ///
    /// Optional, because a project that does not name itself is still a project — but a
    /// declared one is what a player should see, rather than the label path the game happens
    /// to start at.
    #[serde(default)]
    pub name: Option<String>,
    /// The label the game starts at, written `module.label`.
    ///
    /// This is what makes reachability answerable: without a starting point there is no
    /// such thing as an unreachable label.
    pub entry: String,
    /// The frame the game is designed for (`SCREENS.md §2.6`).
    ///
    /// Optional, defaulting to [`Frame::DEFAULT`].
    #[serde(default)]
    pub size: Frame,
}

impl Manifest {
    /// Reads and validates a manifest.
    ///
    /// The failure is a `String` rather than a diagnostic because a broken manifest means
    /// there is no project to report diagnostics *about*.
    pub fn read(path: &Path) -> Result<Self, String> {
        let text =
            fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;

        let manifest: Self =
            toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;

        if manifest.schema != SCHEMA {
            return Err(format!(
                "{}: schema {} is not supported; this build understands schema {SCHEMA}",
                path.display(),
                manifest.schema
            ));
        }

        Ok(manifest)
    }
}
