//! What can go wrong reading or writing a save.
//!
//! Every variant carries the diagnostic code a user would see, because a save failure is the
//! one error a player actually reads (`RUNTIME.md §6.2` names `E7201` and `E7202`).

use std::fmt;

/// A failure reading or writing a save file.
#[derive(Debug)]
pub enum ReplayError {
    /// The file does not begin with the save magic.
    NotASave,
    /// The file is truncated, or its checksum does not match its payload.
    Corrupt {
        /// The checksum the file carries.
        found: u64,
        /// The checksum its bytes produce.
        computed: u64,
    },
    /// The save is older than the engine and the step that would bridge the gap is missing.
    ///
    /// `from` and `to` name *one* version step, not the whole distance to the engine: when a
    /// chain has `4 → 5` but not `5 → 6`, the gap is `5 → 6`, and that is the migration to
    /// write. A partially-migrated world is never loaded (`RUNTIME.md §6.2`).
    MissingMigration {
        /// The version the world is stuck at.
        from: u16,
        /// The version the missing step would produce.
        to: u16,
    },
    /// The save is newer than the engine.
    FromTheFuture {
        /// The version the save was written at.
        save: u16,
        /// The version the engine is at.
        engine: u16,
    },
    /// The save's schema differs from the build's.
    SchemaMismatch {
        /// The schema the save was written against, as hex.
        saved: String,
        /// The schema this build expects, as hex.
        current: String,
    },
    /// A float the save format cannot hold: `null` in JSON is not a number.
    NonFiniteFloat {
        /// The value's name.
        name: String,
    },
    /// The payload would not encode or decode.
    Codec(String),
    /// The file could not be read or written.
    Io(std::io::Error),
}

impl ReplayError {
    /// The diagnostic code for this failure, `E7201`–`E7204`.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::MissingMigration { .. } => "E7201",
            Self::FromTheFuture { .. } => "E7202",
            Self::NotASave | Self::Corrupt { .. } => "E7203",
            Self::SchemaMismatch { .. } => "E7204",
            Self::NonFiniteFloat { .. } | Self::Codec(_) | Self::Io(_) => "E7203",
        }
    }
}

impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotASave => f.write_str("this file is not a save (no `VSAV` header)"),
            Self::Corrupt { found, computed } => write!(
                f,
                "the save is truncated or altered (checksum {found:#x}, computed {computed:#x})"
            ),
            Self::MissingMigration { from, to } => write!(
                f,
                "no migration from save version {from} to {to}: the save cannot be loaded"
            ),
            Self::FromTheFuture { save, engine } => write!(
                f,
                "this save is from a newer version of the game (save {save}, engine {engine})"
            ),
            Self::SchemaMismatch { saved, current } => write!(
                f,
                "the save's schema does not match this build (saved {saved}, expected {current})"
            ),
            Self::NonFiniteFloat { name } => {
                write!(
                    f,
                    "`{name}` is a non-finite float, which a save cannot hold"
                )
            }
            Self::Codec(message) => write!(f, "the save could not be decoded: {message}"),
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ReplayError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ReplayError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
