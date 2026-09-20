//! What a player does about an action a screen asked for.
//!
//! Split from `play.rs` when the file crossed its budget, and along the seam the two functions describe:
//! `play.rs` is the player's *shape* — what it holds, how it starts, how it steps — and this is the one
//! place an action is answered. It is also where the two halves of the split meet: the screen *stack*
//! carries out what it owns (`Stack::dispatch`, `vela-ui`'s), and what comes back as `NotOurs` is the
//! player's to do — the VM's, the save system's, the window's.

use vela_ui::Done;
use vela_ui::actions::Action as ScreenAction;

use super::{FACE_NAME, Player};

impl Player {
    /// Runs an action a screen asked for, however it asked.
    ///
    /// What the screen *stack* does with an action is `vela-ui`'s (`Stack::dispatch`), because a test
    /// that clicks drives a stack too and "what does `hide` mean" should have one answer. What is left
    /// here is what only a player has: the window, the saves, and the rollback history.
    pub(super) fn run(&mut self, action: ScreenAction) {
        let done = self.overlays.dispatch(
            &action,
            self.screens.sets(),
            self.size,
            self.presenter.text_mut(),
            FACE_NAME,
        );
        match done {
            Done::Opened(name) => println!("screen open {name}"),
            Done::Replaced(name) => println!("screen replace {name}"),
            Done::Missing(name) => println!("screen missing {name}"),
            Done::Closed(name) => println!("screen close {name}"),
            Done::Hidden(name) => println!("screen hide {name}"),
            Done::Set(name) => println!("screen set {name}"),
            Done::Stale(name) => println!("screen stale {name}"),
            Done::Nothing => {}
            Done::NotOurs => self.host_action(action),
        }
    }

    /// The actions a player has and a headless run does not: the VM's, the save system's, the window's.
    fn host_action(&mut self, action: ScreenAction) {
        // The player's settings come first, because they are the one vocabulary whose subject is the
        // *player's* state rather than the story's (`RUNTIME.md §2.1`) — and the file is rewritten at
        // once: a setting a player chose and a crash did not keep is one they have to choose again. Which
        // actions those are is `vela-ui`'s list rather than a second one here: a press that a player
        // treats as a setting and a test does not would write two different worlds.
        if vela_ui::settings::is_write(&action) {
            match vela_ui::settings::write(self.timeline.preferences_mut(), &action) {
                Some(name) => {
                    println!("screen setting {name}");
                    self.save_settings();
                    // And every screen is laid out again, so what the player just changed is what the screens
                    // read: a checkbox draws its own new state, and a text speed a dialogue box reads is the
                    // one they chose (`SCREENS.md §7.1`). The slots are handed over again with them, because
                    // the page a save screen draws is one of these settings.
                    let preferences = self.timeline.world().preferences.clone();
                    self.screens.set_preferences(preferences.clone());
                    self.overlays.set_preferences(preferences);
                    self.refresh_slots();
                }
                // A bundle whose vocabulary moved on, or a value the world cannot hold. Saying so beats
                // a press that quietly did nothing.
                None => println!("screen setting refused: {action}"),
            }
            return;
        }
        match action.name.as_str() {
            "quit" => {
                self.quit = true;
                println!("screen quit");
            }
            "quick_save" => self.save("quick"),
            "quick_load" => self.load("quick"),
            // A slot of the current page. Which of the two it is, is Ren'Py's rule about the screen's
            // name (`vela_ui::actions::file_mode`), and the page is the player's setting — both of them
            // questions the file screen asks by writing the action, never by passing the answers.
            vela_ui::actions::FILE_ACTION => self.file_action(&action),
            vela_ui::actions::FILE_DELETE => self.file_delete(&action),
            "rollback" => self.rollback(),
            // A screen's `jump` begins the story at a label — the one action in the vocabulary whose
            // meaning is entirely the host's (`SCREENS.md §2.7`), because a label is the program's and a
            // screen cannot know what labels a project has.
            "jump" => {
                let Some(label) = action.first() else {
                    println!("screen jump (no label)");
                    return;
                };
                if self.jump(label) {
                    println!("screen jump {label}");
                } else {
                    println!("screen jump {label} (no such label)");
                }
            }
            // The rest need the VM or `World` and are not wired yet — `SCREENS.md §7` says which, and
            // the registry carries the same answer. Saying so beats a button that quietly does
            // nothing, which is the failure that looks like the project's mistake.
            other => println!("screen action {other} (declared, not dispatched)"),
        }
    }
}
