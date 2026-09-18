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

use vela_replay::{Save, Timeline};

use super::Player;

impl Player {
    /// Writes the current state to a slot.
    pub(super) fn save(&mut self, slot: &str) {
        if std::fs::create_dir_all(&self.saves).is_err() {
            println!("save failed: cannot create {}", self.saves.display());
            return;
        }
        let save = Save::new(self.timeline.snapshot(), self.schema.digest(), slot);
        let path = self.saves.join(format!("{slot}.velasave"));
        match save.write_atomic(&path) {
            Ok(()) => println!("save {slot}"),
            Err(error) => println!("save failed: {error}"),
        }
    }

    /// Reads a slot back into the running story, migrating it if it is from an older build.
    pub(super) fn load(&mut self, slot: &str) {
        let path = self.saves.join(format!("{slot}.velasave"));
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
        match Timeline::resume(&self.module, &save.snapshot) {
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
