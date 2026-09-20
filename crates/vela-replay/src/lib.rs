//! Snapshots, rollback, the input log, and the save migration engine.
//!
//! # Owns
//!
//! Recorder, Snapshot, Migrator, save serialization and atomic writes — and the *settings* file, which
//! is the player's state rather than the playthrough's and is persisted the same way (`settings.rs`).
//!
//! # Does not own
//!
//! The interpreter (vela-vm); the state model (vela-world).
//!
//! # Shape of a save
//!
//! ```text
//!   World ──encode──► payload (JSON) ──wrap──► envelope ──rename──► a file
//!                                   magic · version · schema digest · checksum
//! ```
//!
//! The envelope is what makes a save *self-describing*: a load reads the version and the
//! schema digest before it trusts a byte of the payload, so a save from another build is
//! routed to the migration engine (`RUNTIME.md §6`) rather than decoded into garbage.
//!
//! The settings file is the same envelope without the schema digest — a preference has no declared
//! shape, so a file written by a newer build is read by an older one (`RUNTIME.md §2.1`).

mod digest;
mod error;
mod history;
mod migrations;
mod save;
mod schema;
pub mod settings;
mod slots;

pub use digest::{checksum, digest, hex};
pub use error::ReplayError;
pub use history::{DEFAULT_DEPTH, DEFAULT_INTERVAL, Timeline};
pub use migrations::chain::{ChainError, Migration, Migrator};
pub use migrations::registry::chain;
pub use save::{MAGIC, SAVE_VERSION, Save, SaveHeader};
pub use schema::{Entry, Schema};
pub use settings::{SETTINGS_VERSION, Settings};
pub use slots::{SLOT_EXTENSION, Slot, path_of, slot_name, slots};
