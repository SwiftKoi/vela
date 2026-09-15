//! What can go wrong importing an asset or reading a manifest.
//!
//! One error type for the crate (`CONVENTIONS.md §2.1`). Every variant carries the source path
//! the author would have to look at, because an import failure the author cannot locate is an
//! import failure they will work around by deleting the asset.

use std::fmt;
use std::path::PathBuf;

/// A failure importing an asset or reading a manifest.
#[derive(Debug)]
pub enum AssetError {
    /// A source could not be read or an artifact could not be written.
    Io {
        /// The file.
        path: PathBuf,
        /// What went wrong.
        source: std::io::Error,
    },
    /// No importer claims this source.
    Unsupported {
        /// The file.
        path: PathBuf,
    },
    /// An importer refused the source it claimed.
    Import {
        /// The file.
        path: PathBuf,
        /// The importer.
        kind: &'static str,
        /// Why.
        message: String,
    },
    /// Two sources want the same id.
    ///
    /// Ids come from paths with the extension dropped, so `art/forest.png` and
    /// `art/forest.ktx2` collide. Refusing names both files rather than letting whichever
    /// the directory walk reached second win.
    DuplicateId {
        /// The id both sources claim.
        id: String,
        /// The first source to claim it.
        first: String,
        /// The second.
        second: String,
    },
    /// A manifest could not be decoded.
    Manifest(String),
    /// A patch could not be read, written, or applied.
    Patch(String),
    /// A file could not be decoded.
    Codec(String),
}

impl fmt::Display for AssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Unsupported { path } => {
                write!(f, "{}: no importer handles this file", path.display())
            }
            Self::Import {
                path,
                kind,
                message,
            } => write!(
                f,
                "{}: the {kind} importer refused it: {message}",
                path.display()
            ),
            Self::DuplicateId { id, first, second } => write!(
                f,
                "`{id}` is claimed by both `{first}` and `{second}`; two assets cannot share an id"
            ),
            Self::Manifest(message) => write!(f, "the manifest could not be read: {message}"),
            Self::Patch(message) => write!(f, "{message}"),
            Self::Codec(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for AssetError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<(PathBuf, std::io::Error)> for AssetError {
    fn from((path, source): (PathBuf, std::io::Error)) -> Self {
        Self::Io { path, source }
    }
}
