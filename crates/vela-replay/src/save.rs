//! The save container: a `World`, versioned, checksummed, and written atomically.
//!
//! `RUNTIME.md §5`. The shape is a small binary envelope around a readable payload:
//!
//! ```text
//!   magic "VSAV"  4
//!   save version  2   (little-endian)
//!   schema digest 32
//!   payload           (JSON: the world, the call stack, metadata)
//!   checksum      8   (little-endian, over everything above)
//! ```
//!
//! The envelope is binary so a load can reject a wrong or damaged file *before* parsing
//! anything, and the payload is text so a save can be read, diffed, and migrated by hand.
//! Both halves matter: `RUNTIME.md §5` asks for a save that is "readable", and the checksum
//! only means something if it covers bytes rather than a parsed structure — which is also why
//! a truncated file is caught by the checksum and not by a length field that the truncation
//! would have cut off.

use std::cmp::Ordering;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use vela_vm::Snapshot;
use vela_world::{Value, World};

use crate::digest::{checksum, hex};
use crate::error::ReplayError;
use crate::migrations::chain::Migrator;
use crate::schema::Schema;

/// The four bytes every save starts with.
pub const MAGIC: &[u8; 4] = b"VSAV";

/// The save format version this build writes.
///
/// A bump means the migration engine has a step to run (`RUNTIME.md §6`). Version 2 renamed
/// the story's `trust` to `affection`; version 3 added the suspension anchor to a frame, which
/// is a change to the file rather than to the world, so its step rewrites nothing — and is
/// still a step, because `save_version` is a claim about the file. `tests/golden/saves/` holds
/// a real save for every version, and `Save::load` is what carries an older one forward.
pub const SAVE_VERSION: u16 = 3;

/// Where the payload begins.
const HEADER: usize = 4 + 2 + 32;
/// How many bytes the trailing checksum takes.
const TRAILER: usize = 8;

/// Metadata about a save, none of which the game logic reads.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SaveHeader {
    /// The format version the file was written at.
    pub save_version: u16,
    /// The schema the world was saved against.
    pub schema_digest: [u8; 32],
    /// The build that wrote it, for a bug report.
    pub engine_build: String,
    /// When it was written, in whatever unit the host chose. Display only.
    pub created_at: u64,
    /// The slot it was written to.
    pub slot: String,
}

/// A save: the world, the machine, and the log length at the moment it was taken.
#[derive(Clone, PartialEq, Debug)]
pub struct Save {
    /// Metadata.
    pub header: SaveHeader,
    /// The world and the machine running it — everything needed to resume.
    pub snapshot: Snapshot,
    /// How many answers the input log held when this was taken.
    pub log_len: u64,
}

/// What the payload holds. The version and digest live in the envelope instead, because they
/// have to be readable *before* the payload is parsed.
#[derive(Serialize, Deserialize)]
struct Payload {
    engine_build: String,
    created_at: u64,
    slot: String,
    log_len: u64,
    snapshot: Snapshot,
}

impl Save {
    /// A save of `snapshot` at the start of the current format version.
    #[must_use]
    pub fn new(snapshot: Snapshot, schema_digest: [u8; 32], slot: impl Into<String>) -> Self {
        Self {
            header: SaveHeader {
                save_version: SAVE_VERSION,
                schema_digest,
                engine_build: String::new(),
                created_at: 0,
                slot: slot.into(),
            },
            snapshot,
            log_len: 0,
        }
    }

    /// Encodes the save: the binary envelope around a JSON payload.
    ///
    /// # Errors
    ///
    /// Fails on a non-finite float, which the payload format cannot hold — better a refusal
    /// now than a `null` that fails to load later.
    pub fn to_bytes(&self) -> Result<Vec<u8>, ReplayError> {
        if let Some(name) = non_finite_name(&self.snapshot.world) {
            return Err(ReplayError::NonFiniteFloat { name });
        }

        let payload = Payload {
            engine_build: self.header.engine_build.clone(),
            created_at: self.header.created_at,
            slot: self.header.slot.clone(),
            log_len: self.log_len,
            snapshot: self.snapshot.clone(),
        };
        let body =
            serde_json::to_vec(&payload).map_err(|error| ReplayError::Codec(error.to_string()))?;

        let mut bytes = Vec::with_capacity(HEADER + body.len() + TRAILER);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&self.header.save_version.to_le_bytes());
        bytes.extend_from_slice(&self.header.schema_digest);
        bytes.extend_from_slice(&body);
        let sum = checksum(&bytes);
        bytes.extend_from_slice(&sum.to_le_bytes());
        Ok(bytes)
    }

    /// Decodes a save at the current version.
    ///
    /// The strict path: a save from any other version is refused. [`Save::load`] is the one
    /// that brings an older save forward through the migration chain.
    ///
    /// # Errors
    ///
    /// Fails on a wrong magic, a short or damaged file (`E7203`), or a version the engine
    /// cannot reach (`E7201` older, `E7202` newer).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ReplayError> {
        let decoded = decode(bytes)?;
        match decoded.version.cmp(&SAVE_VERSION) {
            Ordering::Greater => Err(ReplayError::FromTheFuture {
                save: decoded.version,
                engine: SAVE_VERSION,
            }),
            Ordering::Less => Err(ReplayError::MissingMigration {
                from: decoded.version,
                to: decoded.version.saturating_add(1),
            }),
            Ordering::Equal => {
                let digest = decoded.schema_digest;
                Ok(decoded.at(SAVE_VERSION, digest))
            }
        }
    }

    /// Decodes a save and brings an older one up to the current version.
    ///
    /// `RUNTIME.md §6.2`. A save at the current version must match `schema` (`E7204`); an
    /// older one is rewritten by `migrator` until it does. The result always carries the
    /// current version and schema digest, so whatever a caller holds is current — an older
    /// world never leaves this function.
    ///
    /// # Errors
    ///
    /// Fails for every reason [`Save::from_bytes`] does, and with `E7201` naming the exact
    /// step when the chain has a gap it cannot cross.
    pub fn load(bytes: &[u8], migrator: &Migrator, schema: &Schema) -> Result<Self, ReplayError> {
        let mut decoded = decode(bytes)?;
        let current = schema.digest();
        if decoded.version > SAVE_VERSION {
            return Err(ReplayError::FromTheFuture {
                save: decoded.version,
                engine: SAVE_VERSION,
            });
        }
        if decoded.version < SAVE_VERSION {
            migrator.migrate(
                &mut decoded.payload.snapshot.world,
                schema,
                decoded.version,
                SAVE_VERSION,
            )?;
        } else if decoded.schema_digest != current {
            return Err(ReplayError::SchemaMismatch {
                saved: hex(&decoded.schema_digest),
                current: hex(&current),
            });
        }
        Ok(decoded.at(SAVE_VERSION, current))
    }

    /// The version a save file was written at, read from its header alone.
    ///
    /// For a caller that wants to say *what* a load migrated before it happens. The payload is
    /// not decoded and the checksum is not checked, so this is a label, not a validation.
    ///
    /// # Errors
    ///
    /// Fails if the file is too short to hold a header, or does not start with the magic.
    pub fn version_of(bytes: &[u8]) -> Result<u16, ReplayError> {
        if bytes.len() < HEADER || &bytes[..4] != MAGIC {
            return Err(ReplayError::NotRecognised { what: "a save" });
        }
        Ok(u16::from_le_bytes([bytes[4], bytes[5]]))
    }

    /// Checks the save was written against `schema`.
    ///
    /// # Errors
    ///
    /// Fails with `E7204` when the digests differ: the world would decode, but into a shape
    /// the program does not expect.
    pub fn expect_schema(&self, schema: &Schema) -> Result<(), ReplayError> {
        let current = schema.digest();
        if self.header.schema_digest == current {
            return Ok(());
        }
        Err(ReplayError::SchemaMismatch {
            saved: hex(&self.header.schema_digest),
            current: hex(&current),
        })
    }

    /// Reads and checks a save file.
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be read, or for any reason `from_bytes` does.
    pub fn read(path: &Path) -> Result<Self, ReplayError> {
        Self::from_bytes(&fs::read(path)?)
    }

    /// Reads a save's metadata without decoding its world.
    ///
    /// The question a *list* of slots asks (`crate::slots`, `RUNTIME.md §5`): what is in each slot,
    /// before any of them is loaded. The envelope and the checksum are checked, so a damaged file is an
    /// error rather than a slot that looks healthy; the payload is then read for its metadata alone. No
    /// schema, no migrator, no `World` — a version this build cannot load still has a name and a time,
    /// which is what a player needs to be able to see the slot and delete it.
    ///
    /// # Errors
    ///
    /// Fails for anything that makes the file not a save: short, a wrong magic, a checksum that does not
    /// match, or a payload that does not parse.
    pub fn header_of(bytes: &[u8]) -> Result<SaveHeader, ReplayError> {
        let (save_version, schema_digest, body) = envelope(bytes)?;
        let meta: Meta =
            serde_json::from_slice(body).map_err(|error| ReplayError::Codec(error.to_string()))?;
        Ok(SaveHeader {
            save_version,
            schema_digest,
            engine_build: meta.engine_build,
            created_at: meta.created_at,
            slot: meta.slot,
        })
    }

    /// The metadata of the save at `path`, without loading it.
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be read, or for any reason [`header_of`](Self::header_of) does.
    pub fn header_at(path: &Path) -> Result<SaveHeader, ReplayError> {
        Self::header_of(&fs::read(path)?)
    }

    /// Writes the save to `path`, atomically.
    ///
    /// A temporary file in the same directory is written and flushed, then renamed over the
    /// target. A crash mid-write leaves the previous slot intact, which is the property
    /// `RUNTIME.md §5` asks for and the reason this is not a plain `fs::write`.
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

/// A save as it came off disk: the envelope, and a payload nothing has validated yet.
struct Decoded {
    /// The version the file was written at.
    version: u16,
    /// The schema digest the file carries.
    schema_digest: [u8; 32],
    /// The payload.
    payload: Payload,
}

impl Decoded {
    /// Rebuilds the save as `version` and `schema_digest` — what a caller should see.
    ///
    /// A load that migrated passes the *current* pair, which is what makes the value it
    /// returns current even though the bytes it read were not.
    fn at(self, version: u16, schema_digest: [u8; 32]) -> Save {
        let Payload {
            engine_build,
            created_at,
            slot,
            log_len,
            snapshot,
        } = self.payload;
        Save {
            header: SaveHeader {
                save_version: version,
                schema_digest,
                engine_build,
                created_at,
                slot,
            },
            snapshot,
            log_len,
        }
    }
}

/// Reads the envelope and the payload, checking only what the bytes can prove.
///
/// The magic and the checksum are checked here, before anything is parsed. The *version* is
/// deliberately not: what to do with a version is the caller's question — [`Save::from_bytes`]
/// refuses anything but current, and [`Save::load`] migrates.
fn decode(bytes: &[u8]) -> Result<Decoded, ReplayError> {
    let (version, schema_digest, body) = envelope(bytes)?;
    let payload: Payload =
        serde_json::from_slice(body).map_err(|error| ReplayError::Codec(error.to_string()))?;

    Ok(Decoded {
        version,
        schema_digest,
        payload,
    })
}

/// The envelope: its version, its schema digest, and the payload's bytes.
///
/// Split out of [`decode`] when a *list* of slots needed a save's metadata without its world. One
/// implementation for both readers, because the questions the envelope answers — is this a save at all,
/// is it short, does the checksum hold — have one answer, and a second copy is a second answer.
fn envelope(bytes: &[u8]) -> Result<(u16, [u8; 32], &[u8]), ReplayError> {
    if bytes.len() < HEADER + TRAILER {
        return Err(ReplayError::NotRecognised { what: "a save" });
    }
    if &bytes[..4] != MAGIC {
        return Err(ReplayError::NotRecognised { what: "a save" });
    }

    let body_end = bytes.len() - TRAILER;
    let found = u64::from_le_bytes(bytes[body_end..].try_into().unwrap_or([0; 8]));
    let computed = checksum(&bytes[..body_end]);
    if found != computed {
        return Err(ReplayError::Corrupt { found, computed });
    }

    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    let mut schema_digest = [0u8; 32];
    schema_digest.copy_from_slice(&bytes[6..38]);
    Ok((version, schema_digest, &bytes[HEADER..body_end]))
}

/// The payload's metadata, for a reader that wants no part of the world.
///
/// Deserializing *this* rather than [`Payload`] is the point rather than an optimization: a slot's
/// metadata is what a list shows, and a payload whose world this build cannot read is still a slot with
/// a name, a time, and a version. Every field defaults, so a payload written before one of them existed
/// still describes its slot.
#[derive(Deserialize)]
struct Meta {
    #[serde(default)]
    engine_build: String,
    #[serde(default)]
    created_at: u64,
    #[serde(default)]
    slot: String,
}

/// The name of the first non-finite float in the world, if there is one.
fn non_finite_name(world: &World) -> Option<String> {
    world
        .iter()
        .find_map(|(name, value)| value_has_non_finite(value).then(|| name.clone()))
}

/// Whether a value holds a float that JSON cannot represent.
fn value_has_non_finite(value: &Value) -> bool {
    match value {
        Value::Float(number) => !number.is_finite(),
        Value::List(items) => items.iter().any(value_has_non_finite),
        Value::Map(entries) => entries.values().any(value_has_non_finite),
        Value::Struct { fields, .. } => fields.iter().any(|(_, value)| value_has_non_finite(value)),
        Value::Enum { fields, .. } => fields.iter().any(value_has_non_finite),
        _ => false,
    }
}
