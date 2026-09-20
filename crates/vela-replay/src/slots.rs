//! The directory of slots: what each one holds, without loading any of them.
//!
//! `RUNTIME.md §5`. A save screen draws a *list* before it draws a world — which slots exist, what each
//! was written to, when that was, and whether this build can load it — and that is a different question
//! from reading a slot: `Save::read` wants a schema and a migrator and builds a `World`, none of which a
//! list may need, because a slot whose world is from another build is still a slot the player has to be
//! able to see and delete.
//!
//! So a slot's *file* is the slot. A file whose bytes are not a save at all is listed, unloadable, and
//! keeps its place until somebody deletes it — which is the honest answer, since the alternative is a
//! slot that vanishes from the screen while still occupying the directory.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::ReplayError;
use crate::save::{SAVE_VERSION, Save};

/// The extension a slot's file carries.
pub const SLOT_EXTENSION: &str = "velasave";

/// Where the slot called `name` lives inside `dir`.
#[must_use]
pub fn path_of(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.{SLOT_EXTENSION}"))
}

/// The name a numbered slot is filed under: its page, then the slot — Ren'Py's spelling
/// (`__slotname`), and the one the corpus's `file_slots` grid is built on.
///
/// `1-1` is the first slot of the first page. The name is what a screen's slot number and a file have in
/// common, and the page is a *prefix*, which is why a page's slots are contiguous in the listing `slots`
/// returns and why a screen can draw one page by reading the names that start with it.
///
/// A named slot — `quick`, from `quick_save` — has no page: the page is a *grid position* rather than a
/// namespace, and the actions that write one are the host's (`vela-ui::settings::PAGE`).
#[must_use]
pub fn slot_name(page: u32, slot: u32) -> String {
    format!("{page}-{slot}")
}

/// One slot, as its file describes it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Slot {
    /// The name it is filed under — the file's stem, which is the slot's *position*. The header records
    /// the name too, and a file renamed by hand keeps the header's: the stem is what the screen lists.
    pub name: String,
    /// The page it is on, when its name says one (`1-3` is on page one, slot three).
    ///
    /// Read out of the name rather than stored, because the name *is* the position: `slot_name` writes
    /// both halves, and a screen drawing a page needs to know which of the slots belong to it.
    pub page: Option<u32>,
    /// Its number within that page.
    pub number: Option<u32>,
    /// When it was written, in whatever unit the host chose. Display only (`SaveHeader::created_at`).
    pub time: u64,
    /// The format version the file was written at, when its envelope could be read.
    pub version: Option<u16>,
    /// Whether this build can load it: the envelope and its checksum hold, and the version is not from
    /// the future. An older version is loadable — `Save::load` carries it forward.
    pub loadable: bool,
}

/// The page and number a slot's name says, when it says one.
///
/// `1-3` is page one, slot three; `quick` — what `quick_save` writes — is a *named* slot with no
/// position, which is the difference `slot_name` describes. A name that is neither is nobody's position:
/// `None` here, and the slot is simply not on any page.
#[must_use]
pub fn numbered(name: &str) -> Option<(u32, u32)> {
    let (page, number) = name.split_once('-')?;
    Some((page.parse().ok()?, number.parse().ok()?))
}

/// Every slot in `dir`, by name.
///
/// A directory that does not exist is *no slots* rather than an error: a project nobody has saved from
/// has none, and an empty page is the first thing a save screen has to draw.
///
/// # Errors
///
/// Fails only when the directory exists and cannot be read.
pub fn slots(dir: &Path) -> Result<Vec<Slot>, ReplayError> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut found = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some(SLOT_EXTENSION) {
            continue;
        }
        found.push(slot_of(&path));
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}

/// One file, as a slot.
///
/// The version comes from `Save::version_of` even when the metadata read fails, because its whole point
/// is to read the envelope without validating it: a message about a half-written save can then say what
/// version it was being written at.
fn slot_of(path: &Path) -> Slot {
    let name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_string();
    let (page, number) =
        numbered(&name).map_or((None, None), |(page, number)| (Some(page), Some(number)));
    let Ok(bytes) = fs::read(path) else {
        return Slot {
            name,
            page,
            number,
            time: 0,
            version: None,
            loadable: false,
        };
    };
    let version = Save::version_of(&bytes).ok();
    match Save::header_of(&bytes) {
        Ok(header) => Slot {
            name,
            page,
            number,
            time: header.created_at,
            version: Some(header.save_version),
            loadable: header.save_version <= SAVE_VERSION,
        },
        Err(_) => Slot {
            name,
            page,
            number,
            time: 0,
            version,
            loadable: false,
        },
    }
}
