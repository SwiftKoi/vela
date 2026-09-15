//! Failures reading a compiled screen pack (`SCREENS.md §13`).

use std::fmt;

/// Why a screen pack could not be read.
#[derive(Debug)]
pub enum PackError {
    /// The file is not a screen pack this build can read.
    Malformed(String),
    /// The pack's format version is not one this build reads.
    ///
    /// Refused rather than read as though the fields it does not recognize were absent: a newer
    /// pack may mean something different by a field this build thinks it knows.
    Version(u32),
    /// The file could not be read or written.
    Io {
        /// The path it tried.
        path: std::path::PathBuf,
        /// What the operating system said.
        source: std::io::Error,
    },
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(message) => write!(f, "the screen pack is not readable: {message}"),
            Self::Version(version) => write!(
                f,
                "screen pack version {version} is not one this build reads; rebuild the bundle"
            ),
            Self::Io { path, source } => write!(f, "cannot read {}: {source}", path.display()),
        }
    }
}

impl std::error::Error for PackError {}
