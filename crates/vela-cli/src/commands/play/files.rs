//! The actions a file screen presses: a slot, on the page the player is looking at.
//!
//! Split from `saves.rs`, which is the two primitives that touch a slot's *bytes*. What a `file_action`
//! is depends on which screen asked (Ren'Py's rule, and `vela_ui::actions::file_mode` is where it is
//! written down) and on which page the player is on (the player's setting, `vela-ui::settings::page`) —
//! both of them policy the store knows nothing about.
//!
//! The page is what makes a slot *name*: a screen writes `file_action(3)` for the third slot of the page
//! it is drawing, and the file it lands in is `{page}-3.velasave` (`vela_replay::slot_name`).

use vela_replay::slot_name;
use vela_ui::actions::{FileMode, file_mode};
use vela_ui::settings;
use vela_ui::{Action, Value};

use super::Player;

impl Player {
    /// `file_action(slot)`: saves into a slot or loads from it, by which screen asked.
    pub(super) fn file_action(&mut self, action: &Action) {
        // Ren'Py's `FileAction`: `load` when the screen that asked is named `load`, save otherwise. The
        // stack knows the name because it is the screen that dispatched the action.
        let mode = file_mode(self.overlays.top().map(|top| top.name.as_str()));
        let Some(name) = self.slot(action) else {
            println!("screen file: no slot");
            return;
        };
        match mode {
            FileMode::Save => {
                self.save(&name);
                // The set changed, so what a save screen draws changed with it (`Player::refresh_slots`).
                self.refresh_slots();
            }
            FileMode::Load => self.load(&name),
        }
    }

    /// `file_delete(slot)`: removes what a slot holds, and says which one it was.
    pub(super) fn file_delete(&mut self, action: &Action) {
        let Some(name) = self.slot(action) else {
            println!("screen file: no slot");
            return;
        };
        let path = vela_replay::path_of(&self.saves, &name);
        match std::fs::remove_file(&path) {
            Ok(()) => println!("delete {name}"),
            // A slot that is already empty is not a failure: the screen's button says "delete what is
            // there", and nothing being there is that wish granted. Saying so rather than printing an
            // error keeps a page of empty slots from looking broken.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                println!("delete {name} (empty)");
            }
            Err(error) => println!("delete failed: {error}"),
        }
        // Whether it removed a file or found none, the page is not what it was: a deleted slot draws as an
        // empty cell, which is the cell a player saves into next.
        self.refresh_slots();
    }

    /// The slot a file action names, as the file it lives in.
    ///
    /// `None` when the action carries no number — which a screen cannot write (the checker holds the
    /// argument to a number, `E5013`) but a bundle from another build might.
    fn slot(&self, action: &Action) -> Option<String> {
        let Some(Value::Num(slot)) = action.args.first() else {
            return None;
        };
        Some(slot_name(
            settings::page(&self.timeline.world().preferences),
            *slot as u32,
        ))
    }
}
