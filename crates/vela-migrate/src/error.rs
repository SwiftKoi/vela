//! Why a migration could not finish.

use std::path::PathBuf;

/// A failure of the migration itself, as opposed to something it reports about a project.
///
/// The split matters: "this construct has no Vela equivalent" is a *report entry* — the
/// migration succeeded and the output says what to do by hand. "This file could not be read" is
/// an error, because there is nothing to report about a file that was not there.
#[derive(Debug)]
pub enum MigrateError {
    /// A path could not be read or written.
    Io {
        /// The path it tried.
        path: PathBuf,
        /// What the operating system said.
        source: std::io::Error,
    },
    /// The path is not a Ren'Py project: it has no `game/` directory.
    NotARenpyProject(PathBuf),
}

impl std::fmt::Display for MigrateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "cannot use {}: {source}", path.display()),
            Self::NotARenpyProject(path) => write!(
                f,
                "`{}` is not a Ren'Py project: it has no `game/` directory",
                path.display()
            ),
        }
    }
}

impl std::error::Error for MigrateError {}

impl From<std::io::Error> for MigrateError {
    /// An I/O failure with no path attached.
    ///
    /// Callers that know the path use [`MigrateError::Io`] directly; this is for the few
    /// operations that do not, and it exists so `?` works rather than to encourage losing the
    /// path.
    fn from(source: std::io::Error) -> Self {
        Self::Io {
            path: PathBuf::new(),
            source,
        }
    }
}
