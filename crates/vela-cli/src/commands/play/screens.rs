//! What a player does with the screens a story opens, and with the label one of them names.
//!
//! Split from `play.rs` when the doorway's `jump` pushed that file past its budget, and the cut is the
//! one the methods already describe: they are the player's half of the screen vocabulary — carry out what
//! the focused control asked for, open and close the menu, and begin the story at a label. What a *stack*
//! does with an action is `vela-ui`'s (`Stack::dispatch`); what is left here is what only a player has.

use super::{FACE_NAME, Player};
use vela_replay::Timeline;

impl Player {
    /// Starts the story at a label, which is what a screen's `jump` means to a player.
    ///
    /// The label resolves in the module the game's entry point is in, because that is what a screen can
    /// name without knowing how a project is laid out: an interface's `Start` is `jump(start)`, and a
    /// game's first label is conventionally `start` (`SCREENS.md §2.7`). A label in *another* module is
    /// therefore not reachable this way, which is the right shape — a menu button names a beginning, and
    /// a cross-module transfer is the story's business (`jump` in a script).
    ///
    /// The open screens are closed first: a menu that stayed over the first line of the new game would be
    /// a menu the player cannot leave, and starting means the menu is done. Answers `false` for a label
    /// the program does not have, so a caller can say so rather than sit on a frame that will not move.
    pub(crate) fn jump(&mut self, label: &str) -> bool {
        // A bare name is the entry module's: `jump(start)` in a screen the interface wrote, for a game
        // whose entry point is `script.start`, means `script.start`. A name with a dot is taken as written,
        // so a project can still reach the rest of its program.
        let label = match label.contains('.') {
            true => label.to_string(),
            false => format!("{}.{label}", self.entry_module),
        };
        let Ok(timeline) = Timeline::start(&self.module, &label) else {
            return false;
        };
        while self.overlays.close().is_some() {}
        self.timeline = timeline;
        self.finished = false;
        true
    }

    /// Escape: close the top screen, or open `pause` when there is nothing to close.
    ///
    /// The "or open the menu" half of the host's `Cancel` binding. A project without a `pause`
    /// screen simply has no menu, which is the one-line script staying a one-line script.
    pub(super) fn toggle_menu(&mut self) {
        if let Some(name) = self.overlays.close() {
            println!("screen close {name}");
            return;
        }
        if !self.screens.has("pause") {
            return;
        }
        if self.overlays.open(
            self.screens.sets(),
            "pause",
            &[],
            self.size,
            self.presenter.text_mut(),
            FACE_NAME,
        ) {
            println!("screen open pause");
        }
    }

    /// Activates the focused hotspot's action.
    pub(super) fn activate(&mut self) {
        let Some(action) = self.overlays.focused().cloned() else {
            return;
        };
        println!("screen activate {action}");
        self.run(action);
    }
}
