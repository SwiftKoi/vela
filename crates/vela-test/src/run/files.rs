//! The file actions a run carries out: a slot is a real file, in the test's own directory.
//!
//! Item 5's evidence is end-to-end — "the save screen lists real slots, and loading one returns to the
//! right place" — so a press that saves has to *save*, into a file the run can read back. Every decision
//! inside that is already made and tested elsewhere: which of the two a `file_action` is, is Ren'Py's rule
//! about the screen's name (`vela_ui::actions::file_mode`); which page it lands on is the player's setting
//! (`vela_ui::settings::page`); and the file is named for both (`vela_replay::slot_name`). What is here is
//! the joining of them to a session and a directory.

use vela_replay::{Save, path_of, slot_name};
use vela_ui::actions::{Action, FILE_ACTION, FILE_DELETE, FileMode, file_mode};
use vela_ui::settings;
use vela_ui::value::Value;
use vela_vm::Session;

use crate::report::Reason;
use crate::saves::Saves;
use crate::stage::Stage;

/// Reads the run's directory and tells the stage what is in it (`vela-ui::slots`).
///
/// The same hand-over the windowed player makes, for the same reason: a screen cannot ask the file system,
/// so `slots(6)` answers what the run last found — which is why this is called when the set changes, and why
/// the stage is laid out again: a save screen that did not redraw would show the page as it was before the
/// press.
pub(super) fn refresh(stage: &mut Stage<'_>, saves: &Saves) {
    // A directory that cannot be read is *no slots* rather than a step failure: it is the run's own, and a
    // step that saved into it would have failed where it saved.
    let found: Vec<vela_replay::Slot> = vela_replay::slots(saves.dir()).unwrap_or_default();
    let offered: Vec<vela_ui::Slot> = found.iter().filter_map(offered).collect();
    stage.set_slots(offered);
    stage.relaid();
}

/// One store slot as the screen-facing data a save screen reads (`vela_ui::slots::Slot`).
///
/// `None` for a slot with no position — `quick`, which `quick_save` writes, is not a cell of a page and so is
/// not part of what `slots(6)` answers.
fn offered(slot: &vela_replay::Slot) -> Option<vela_ui::Slot> {
    Some(vela_ui::Slot {
        page: slot.page?,
        number: slot.number?,
        name: slot.name.clone(),
        time: slot.time,
        loadable: slot.loadable,
    })
}

/// Carries out a file action against a test's own slots, answering whether it was one.
///
/// `None` is "not a file action", which is the caller's to hand to the stack. `Some(Err(..))` is a slot
/// that could not be read or written — a real failure, since a run that saved nothing and said nothing
/// would leave a test asserting about a file that is not there.
pub(super) fn carry_out(
    session: &mut Session,
    saves: &Saves,
    top: Option<&str>,
    action: &Action,
) -> Option<Result<(), Reason>> {
    match action.name.as_str() {
        FILE_ACTION => {
            let name = self::name(session, action)?;
            Some(match file_mode(top) {
                FileMode::Save => write(saves, session, &name),
                FileMode::Load => load(saves, session, &name),
            })
        }
        FILE_DELETE => {
            let name = self::name(session, action)?;
            let path = path_of(saves.dir(), &name);
            Some(match std::fs::remove_file(&path) {
                Ok(()) => Ok(()),
                // Deleting what is not there is the wish granted rather than an error: the button says
                // "delete what is in this slot", and a page of empty slots is not a broken page.
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(failed(&error)),
            })
        }
        _ => None,
    }
}

/// The slot a file action names, as the file it lives in.
///
/// `None` for an action that names no slot: a screen cannot write one (the checker holds the argument to
/// a number, `E5013`) but a bundle from another build might, and there is nothing to do about it either
/// way.
fn name(session: &Session, action: &Action) -> Option<String> {
    let Some(Value::Num(slot)) = action.args.first() else {
        return None;
    };
    let page = settings::page(&session.world().preferences);
    Some(slot_name(page, *slot as u32))
}

/// Writes the session's state into a slot.
fn write(saves: &Saves, session: &Session, name: &str) -> Result<(), Reason> {
    if let Err(error) = std::fs::create_dir_all(saves.dir()) {
        return Err(failed(&error));
    }
    let mut save = Save::new(session.snapshot(), saves.schema().digest(), name);
    // The platform's clock, the same one the windowed player stamps with: a run is a host for the file
    // actions, and a slot a *test* writes should be as real as a slot a player writes (`RUNTIME.md §5`).
    // `vela-test` calls the host rather than reading the clock itself, which is the rule the
    // determinism check exists to keep.
    save.header.created_at = vela_host::stamp();
    save.write_atomic(&path_of(saves.dir(), name))
        .map_err(|error| failed(&error))
}

/// Reads a slot back into the session.
///
/// An empty slot is Ren'Py's *insensitive* button: the press carries out nothing rather than failing,
/// which is why an absent file answers `Ok`. The alternative — a failure for "the test pressed a load
/// button on an empty slot" — would report the empty slot as the mistake, when what the test meant to
/// check is what the screen shows after the press, and its own next assertion is what says so.
fn load(saves: &Saves, session: &mut Session, name: &str) -> Result<(), Reason> {
    let Ok(bytes) = std::fs::read(path_of(saves.dir(), name)) else {
        return Ok(());
    };
    let save = Save::load(&bytes, &vela_replay::chain(), saves.schema())
        .map_err(|error| failed(&error))?;
    let preferences = session.world().preferences.clone();
    session
        .resume(&save.snapshot, preferences)
        .map_err(|fault| failed(&fault))
}

/// A slot that could not be read or written.
fn failed(error: &impl std::fmt::Display) -> Reason {
    Reason::File {
        message: error.to_string(),
    }
}
