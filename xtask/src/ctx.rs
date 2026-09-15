//! The shared execution context: where the workspace lives and how to describe paths.

use std::path::{Path, PathBuf};

/// Execution context passed to every check.
#[derive(Debug)]
pub struct Ctx {
    /// Workspace root; all paths reported in diagnostics are relative to this.
    pub root: PathBuf,
}

impl Ctx {
    /// Builds a context from the current directory.
    ///
    /// `cargo xtask` always runs with the workspace root as the working directory, so
    /// no searching upward is required.
    #[must_use]
    pub fn new() -> Self {
        Self {
            root: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        }
    }

    /// Formats a path relative to the workspace root, for stable diagnostic output.
    #[must_use]
    pub fn rel(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }
}

impl Default for Ctx {
    fn default() -> Self {
        Self::new()
    }
}
