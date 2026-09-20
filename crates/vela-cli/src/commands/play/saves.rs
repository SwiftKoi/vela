//! Writing a slot, and reading one back (`RUNTIME.md §5`, `§6`).
//!
//! Split from `play.rs` because a save slot is the one part of playing that touches the filesystem: what
//! a slot *is* belongs to `vela-replay`, and what a running player does with one is two methods with
//! their own failure stories — a directory that cannot be made, a file that is not a save, a save from a
//! build that has moved on.
//!
//! A child module of `play`, so it reads the player's fields without widening them: a save is written
//! *from* the running story, and paperwork that had to be handed a copy of the timeline would be a
//! second thing to keep in step.

use vela_replay::{Save, Timeline, path_of};

use super::{FACE_NAME, Player};

impl Player {
    /// Writes the current state to a slot.
    pub(super) fn save(&mut self, slot: &str) {
        if std::fs::create_dir_all(&self.saves).is_err() {
            println!("save failed: cannot create {}", self.saves.display());
            return;
        }
        let mut save = Save::new(self.timeline.snapshot(), self.schema.digest(), slot);
        // The platform's clock, injected at the one place that writes a real save (`RUNTIME.md §5`):
        // the stamp is display data beside the world, so a slot can say when it was written without
        // anything a story does being able to see it.
        save.header.created_at = vela_host::stamp();
        let path = path_of(&self.saves, slot);
        match save.write_atomic(&path) {
            Ok(()) => println!("save {slot}"),
            Err(error) => println!("save failed: {error}"),
        }
    }

    /// Reads a slot back into the running story, migrating it if it is from an older build.
    pub(super) fn load(&mut self, slot: &str) {
        let path = path_of(&self.saves, slot);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                println!("load failed: {error}");
                return;
            }
        };
        let written = Save::version_of(&bytes).unwrap_or(vela_replay::SAVE_VERSION);
        let save = match Save::load(&bytes, &vela_replay::chain(), &self.schema) {
            Ok(save) => save,
            Err(error) => {
                println!("load failed: {error}");
                return;
            }
        };
        // The player's settings are not in the save (`RUNTIME.md §2.1`): a load restores the story and
        // keeps the settings this session already had, which is why they are handed over here rather
        // than read out of the file.
        let preferences = self.timeline.world().preferences.clone();
        match Timeline::resume(&self.module, &save.snapshot, preferences) {
            Ok(timeline) => {
                self.timeline = timeline;
                self.finished = false;
                self.refresh_presentation();
                if written == vela_replay::SAVE_VERSION {
                    println!("load {slot}");
                } else {
                    println!(
                        "load {slot} (migrated {written} -> {})",
                        vela_replay::SAVE_VERSION
                    );
                }
            }
            Err(fault) => println!("load failed: {fault}"),
        }
    }
}

impl super::Player {
    /// Steps back one command, replaying from the nearest snapshot.
    pub(super) fn rollback(&mut self) {
        let position = self.timeline.position();
        if position == 0 {
            return;
        }
        let reached = self.timeline.rollback(position - 1);
        self.finished = false;
        self.refresh_presentation();
        println!("rollback {reached}");
    }

    /// Re-applies the command now on screen, so the presenter matches a restored state.
    ///
    /// The scene is *not* rebuilt: the presenter's staged images come from the command stream,
    /// and a rollback or load only re-applies the current command. A rollback across a `scene`
    /// change therefore leaves the old backdrop. Stated rather than implied.
    pub(super) fn refresh_presentation(&mut self) {
        if let Some(command) = self.timeline.current() {
            self.presenter.apply(command);
        }
    }
}

impl super::Player {
    /// One store slot as the screen-facing data a save screen reads (`vela-ui::slots::Slot`).
    ///
    /// `None` for a slot with no position — `quick`, which `quick_save` writes, is not a cell of a page and
    /// so is not part of what `slots(6)` answers.
    fn offered(slot: &vela_replay::Slot) -> Option<vela_ui::Slot> {
        Some(vela_ui::Slot {
            page: slot.page?,
            number: slot.number?,
            name: slot.name.clone(),
            time: slot.time,
            loadable: slot.loadable,
        })
    }

    /// Reads the saves directory and tells every screen what is in it (`vela-ui::slots`).
    ///
    /// A screen cannot ask the file system, so `slots(6)` answers what the host last handed over — which
    /// means the hand-over is refreshed whenever the set changes: once at startup, after a save, and after a
    /// delete. The stack is laid out again for the same reason a settings press re-lays it: a save screen
    /// that did not redraw would show the page as it was before the press.
    pub(in crate::commands) fn refresh_slots(&mut self) {
        let found = match vela_replay::slots(&self.saves) {
            Ok(found) => found,
            Err(error) => {
                // Said rather than drawn as an empty page: a save screen showing no slots would be a lie
                // about the player's own files, and the store's words are the honest answer.
                println!("saves unreadable: {error}");
                return;
            }
        };
        let offered: Vec<vela_ui::Slot> = found.iter().filter_map(Self::offered).collect();
        self.screens.set_slots(offered.clone());
        self.overlays.set_slots(offered);
        self.overlays.relaid(
            self.screens.sets(),
            self.size,
            self.presenter.text_mut(),
            FACE_NAME,
        );
    }
}
