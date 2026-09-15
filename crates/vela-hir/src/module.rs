//! Module names.

use std::fmt;
use std::path::Path;

/// A module's dotted name, derived from its path under the source root.
///
/// `chapters/forest.vela` is `chapters.forest`. A module name is a module's *identity*:
/// it is what `use` names and what qualifies a label (`forest.clearing`), so two files
/// cannot share one. That is the point — a flat global label namespace is one of the
/// failures `LANGUAGE.md §6` breaks with.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ModuleName(String);

impl ModuleName {
    /// Derives a module name from a path relative to the source root.
    ///
    /// Returns `None` when the path is not a `.vela` file, so a stray directory entry
    /// cannot quietly become a module.
    #[must_use]
    pub fn from_path(relative: &Path) -> Option<Self> {
        if relative.extension()? != "vela" {
            return None;
        }

        let mut segments = Vec::new();
        for component in relative.with_extension("").components() {
            let segment = component.as_os_str().to_str()?;
            if segment.is_empty() {
                return None;
            }
            segments.push(segment.to_string());
        }

        if segments.is_empty() {
            return None;
        }
        Some(Self(segments.join(".")))
    }

    /// Creates a name directly, for fixtures and for callers that already have one.
    #[must_use]
    pub fn new(dotted: impl Into<String>) -> Self {
        Self(dotted.into())
    }

    /// The dotted form, as written in `use` and in a qualified reference.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The name's segments: `chapters.forest` yields `chapters`, then `forest`.
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.0.split('.')
    }

    /// The last segment, which is the name a sibling module would use unqualified.
    #[must_use]
    pub fn last_segment(&self) -> &str {
        self.0.rsplit('.').next().unwrap_or(&self.0)
    }
}

impl fmt::Display for ModuleName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
