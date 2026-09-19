//! The settings file: the player's `Preferences`, versioned and checksummed like a save.
//!
//! `RUNTIME.md §2.1` says what a setting *is* — the player's state, in no save and undone by no
//! rollback — and this is where one *lives* between sessions. It borrows the save's shape for the save's
//! reasons (`RUNTIME.md §5`): a binary envelope so a wrong or damaged file is refused before anything is
//! parsed, a readable payload so a player can look at their own settings, and a checksum over bytes
//! rather than over a parsed structure. A rename in the store is a version bump with a step, the same
//! way a `default` rename is for a save (`RUNTIME.md §6`), and a gap in that chain is `E7201` naming the
//! exact step — which is what a player sees instead of preferences silently reset to the defaults.
//!
//! It is deliberately **not** a save: a different magic, so the two files can never be mistaken for one
//! another, and no schema digest, because a preference has no declared *shape* — the vocabulary names
//! them (`SCREENS.md §7.1`) and the store holds any name — so a file written by a newer build is read by
//! an older one rather than refused.

use std::cmp::Ordering;
use std::fs;
use std::path::Path;

use vela_world::Preferences;

use crate::digest::checksum;
use crate::error::ReplayError;

/// The four bytes every settings file starts with.
pub const MAGIC: &[u8; 4] = b"VPRE";

/// The settings format version this build writes.
///
/// A bump means the migration table has a step to run (`RUNTIME.md §6`), and
/// `tests/golden/settings/` keeps a real file for every version the format has had.
pub const SETTINGS_VERSION: u16 = 1;

/// The file's name, inside the directory a game keeps its saves in.
///
/// A player's settings sit beside their saves for the reason Ren'Py's `persistent` does: the two travel
/// together — a save without the settings that wrote it is still loadable, and a settings file with no
/// saves is a game that has been configured and not yet played.
pub const FILE_NAME: &str = "settings.velaprefs";

/// Where the payload begins.
const HEADER: usize = 4 + 2;
/// How many bytes the trailing checksum takes.
const TRAILER: usize = 8;

/// The player's settings, as they live in a file.
#[derive(Clone, PartialEq, Debug)]
pub struct Settings {
    /// The version the file was written at, or the current one for a value that was never read.
    pub version: u16,
    /// What the player chose.
    pub preferences: Preferences,
}

impl Settings {
    /// The settings a player who has chosen nothing has: an empty store, at the current version.
    #[must_use]
    pub fn new(preferences: Preferences) -> Self {
        Self {
            version: SETTINGS_VERSION,
            preferences,
        }
    }

    /// Encodes the file: the binary envelope around a readable payload.
    ///
    /// # Errors
    ///
    /// Fails on a non-finite float, which the payload format cannot hold — a preference that cannot be
    /// written is better refused than silently dropped on the way in.
    pub fn to_bytes(&self) -> Result<Vec<u8>, ReplayError> {
        if let Some(name) = self
            .preferences
            .iter()
            .find(|(_, value)| has_non_finite(value))
            .map(|(name, _)| name.clone())
        {
            return Err(ReplayError::NonFiniteFloat { name });
        }

        let body = serde_json::to_vec(&self.preferences)
            .map_err(|error| ReplayError::Codec(error.to_string()))?;

        let mut bytes = Vec::with_capacity(HEADER + body.len() + TRAILER);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&self.version.to_le_bytes());
        bytes.extend_from_slice(&body);
        let sum = checksum(&bytes);
        bytes.extend_from_slice(&sum.to_le_bytes());
        Ok(bytes)
    }

    /// Decodes a file and brings an older one up to the current version.
    ///
    /// The strict path is deliberately missing: a settings file is read at *startup*, where the answer
    /// to "this is from an older build" is a migration rather than a refusal (`RUNTIME.md §6.2`).
    ///
    /// # Errors
    ///
    /// Fails on a wrong magic, a short or damaged file (`E7203`), a version the engine cannot reach
    /// (`E7201` older, `E7202` newer), or a payload that does not decode.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ReplayError> {
        let (version, preferences) = decode(bytes)?;
        match version.cmp(&SETTINGS_VERSION) {
            Ordering::Greater => Err(ReplayError::FromTheFuture {
                save: version,
                engine: SETTINGS_VERSION,
            }),
            Ordering::Equal => Ok(Self {
                version,
                preferences,
            }),
            Ordering::Less => {
                let mut preferences = preferences;
                migrate(&mut preferences, version, SETTINGS_VERSION)?;
                Ok(Self {
                    version: SETTINGS_VERSION,
                    preferences,
                })
            }
        }
    }

    /// The version a file was written at, read from its header alone.
    ///
    /// For a caller that wants to say *what* it migrated before it happens. The payload is not decoded
    /// and the checksum is not checked, so this is a label rather than a validation.
    ///
    /// # Errors
    ///
    /// Fails if the file is too short to hold a header, or does not start with the magic.
    pub fn version_of(bytes: &[u8]) -> Result<u16, ReplayError> {
        if bytes.len() < HEADER || &bytes[..4] != MAGIC {
            return Err(ReplayError::NotRecognised { what: "a settings" });
        }
        Ok(u16::from_le_bytes([bytes[4], bytes[5]]))
    }

    /// Reads and checks a settings file.
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be read, or for any reason [`Settings::from_bytes`] does.
    pub fn read(path: &Path) -> Result<Self, ReplayError> {
        Self::from_bytes(&fs::read(path)?)
    }

    /// Writes the file to `path`, atomically.
    ///
    /// A temporary file in the same directory is written and flushed, then renamed over the target — the
    /// same property a save has (`RUNTIME.md §5`): a crash mid-write leaves the previous settings
    /// intact, and a player loses a setting rather than all of them.
    ///
    /// # Errors
    ///
    /// Fails if the bytes cannot be encoded, or the file cannot be written or renamed.
    pub fn write_atomic(&self, path: &Path) -> Result<(), ReplayError> {
        let bytes = self.to_bytes()?;
        let temporary = path.with_extension("tmp");
        {
            use std::io::Write;
            let mut file = fs::File::create(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
        }
        fs::rename(&temporary, path)?;
        Ok(())
    }
}

/// Reads the envelope and the payload, checking only what the bytes can prove.
///
/// The magic and the checksum are checked here, before anything is parsed; the *version* is the caller's
/// question, because what to do with one is a decision (`Settings::from_bytes` migrates, `version_of`
/// only labels).
fn decode(bytes: &[u8]) -> Result<(u16, Preferences), ReplayError> {
    if bytes.len() < HEADER + TRAILER || &bytes[..4] != MAGIC {
        return Err(ReplayError::NotRecognised { what: "a settings" });
    }

    let body_end = bytes.len() - TRAILER;
    let found = u64::from_le_bytes(bytes[body_end..].try_into().unwrap_or([0; 8]));
    let computed = checksum(&bytes[..body_end]);
    if found != computed {
        return Err(ReplayError::Corrupt { found, computed });
    }

    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    let preferences: Preferences = serde_json::from_slice(&bytes[HEADER..body_end])
        .map_err(|error| ReplayError::Codec(error.to_string()))?;
    Ok((version, preferences))
}

/// One version step for the settings file: what brings a store from `from` to `to`.
///
/// Declared as a function rather than as the save chain's data (`migrations/chain.rs`) because there is
/// no `Schema` for it to consult: a preference has no declared shape, so a rename, a removal and a
/// default are each one line over a flat map, and a table of *operations* would be more machinery than
/// the whole file. The rules are the same ones, and they are the reason this exists at all: a step per
/// version, no step skipped, and a gap that names itself.
pub struct Migration {
    /// The version it starts from.
    pub from: u16,
    /// The version it produces.
    pub to: u16,
    /// What it does to the store.
    pub rewrite: fn(&mut Preferences),
}

/// Every step this build ships, ordered by the version it starts from.
///
/// `CONVENTIONS.md §4.9`'s recipe, for the settings file: a step is a function beside this list and one
/// line here. There are none yet — version 1 is the first — and the corpus test in
/// `crates/vela-replay/tests/settings.rs` is what fails the day a version is added without one.
#[must_use]
pub fn chain() -> Vec<Migration> {
    Vec::new()
}

/// Applies steps until `preferences` is at `to`.
///
/// # Errors
///
/// Fails with `E7201` naming the exact gap when no step starts from the version the store is at — which
/// is what a player sees instead of a settings file quietly resetting to its defaults.
pub fn migrate(preferences: &mut Preferences, from: u16, to: u16) -> Result<(), ReplayError> {
    let steps = chain();
    let mut version = from;
    while version < to {
        let Some(step) = steps.iter().find(|step| step.from == version) else {
            return Err(ReplayError::MissingMigration {
                from: version,
                to: version.saturating_add(1),
            });
        };
        (step.rewrite)(preferences);
        version = step.to;
    }
    Ok(())
}

/// Whether a value holds a float the payload format cannot represent.
fn has_non_finite(value: &vela_world::Value) -> bool {
    use vela_world::Value;
    match value {
        Value::Float(number) => !number.is_finite(),
        Value::List(items) => items.iter().any(has_non_finite),
        Value::Map(entries) => entries.values().any(has_non_finite),
        Value::Struct { fields, .. } => fields.iter().any(|(_, value)| has_non_finite(value)),
        Value::Enum { fields, .. } => fields.iter().any(has_non_finite),
        _ => false,
    }
}
