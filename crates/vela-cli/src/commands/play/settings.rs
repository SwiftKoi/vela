//! The player's settings, read at startup and written when one changes (`RUNTIME.md §2.1`).
//!
//! Split from `play.rs` the way the saves are (`play/saves.rs`): what a settings *file* is belongs to
//! `vela-replay`, and what a running player does with one is two calls and their failure stories. A
//! setting is the player's state rather than the story's, so these are the two moments it crosses the
//! boundary — the file is read when a session starts, and rewritten the moment a screen changes one.

use crate::commands::play::Player;

/// The player's settings, read from beside their saves.
///
/// A file that is not there is the ordinary case — nobody has chosen anything yet. A file that *will not*
/// read is said out loud and the defaults are kept, because preferences are a convenience and refusing to
/// play over a damaged one would be a worse answer than starting with the engine's own.
pub(crate) fn read_settings(dir: &std::path::Path) -> Option<vela_replay::Settings> {
    let path = dir.join(vela_replay::settings::FILE_NAME);
    if !path.exists() {
        return None;
    }
    match vela_replay::Settings::read(&path) {
        Ok(settings) => Some(settings),
        Err(error) => {
            println!("settings ignored: {error}");
            None
        }
    }
}

/// The player's settings, written beside their saves.
///
/// The directory is made first, the way a save's is: a game whose player has never saved still has
/// settings, and a setting that could not be written for want of a directory would be lost with no reason
/// a player could see.
pub(crate) fn write_settings(
    dir: &std::path::Path,
    preferences: vela_world::Preferences,
) -> Result<(), vela_replay::ReplayError> {
    std::fs::create_dir_all(dir)?;
    vela_replay::Settings::new(preferences)
        .write_atomic(&dir.join(vela_replay::settings::FILE_NAME))
}

impl Player {
    /// Writes the player's settings beside their saves (`RUNTIME.md §2.1`).
    ///
    /// Immediately rather than at exit: a setting a player chose and a crash did not keep is one they have
    /// to choose again, and the file is a few hundred bytes.
    pub(super) fn save_settings(&mut self) {
        if let Err(error) = write_settings(&self.saves, self.timeline.world().preferences.clone()) {
            println!("settings not saved: {error}");
        }
    }
}
