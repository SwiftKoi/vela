//! Reading `vela.toml`.

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

/// The `[project]` table.
#[derive(Debug, Deserialize)]
pub struct Project {
    /// The label the game starts at, written `module.label`.
    ///
    /// This is what makes reachability answerable: without a starting point there is no
    /// such thing as an unreachable label.
    pub entry: String,
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
