//! What a run does with a slot: a real file, in a directory of the test's own.
//!
//! `RUNTIME.md §5` and `SCREENS.md §7`. Item 5's evidence is end to end — *"the save screen lists real
//! slots with real timestamps, and loading one returns to the right place"* — and this is the runner's half
//! of it: a press that saves has to save into a file the store can read back, and a press in a screen named
//! `load` has to come back to the line the slot was taken at.

use std::path::PathBuf;

use vela_replay::{Schema, slots};
use vela_span::FileId;
use vela_text::{Font, TextEngine};
use vela_ui::ScreenSet;

use super::support::{STORY, suite};
use crate::run::run;
use crate::saves::Saves;
use crate::stage::Stage;

/// The screens a save and a load are pressed on: the *same* action in both, which is Ren'Py's shape and
/// the reason the screen's name is what decides which of the two it is (`vela_ui::actions::file_mode`).
const SCREENS: &str = "\
screen pause:
    column gap 12:
        button:
            text \"Save\"
            action open_screen(save)
        button:
            text \"Load\"
            action open_screen(load)

screen save:
    column gap 12:
        for cell in slots(3):
            button:
                action file_action(cell.number)
                if cell.empty:
                    text cell.number
                else:
                    text cell.name
        button:
            text \"Back\"
            action close_screen()

screen load:
    column gap 12:
        for cell in slots(3):
            button:
                action file_action(cell.number)
                if cell.empty:
                    text cell.number
                else:
                    text cell.name
        button:
            text \"Back\"
            action close_screen()
";

/// The fixture's screens, compiled.
fn screens() -> [ScreenSet; 1] {
    let parsed = vela_syntax::parse(FileId::from_raw(0), SCREENS);
    assert!(parsed.diagnostics.is_empty(), "the fixture must parse");
    [ScreenSet::from_items(&parsed.program.items)]
}

/// The schema the fixture's world is saved against, derived from its own source.
fn schema(text: &str) -> Schema {
    let parsed = vela_syntax::parse(FileId::from_raw(0), text);
    Schema::derive(&parsed.program.items)
}

/// The bundled face, because laying a screen out needs one to size text with.
fn engine() -> TextEngine {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fonts/LiberationSans-Regular.ttf");
    let font = Font::from_bytes(std::fs::read(path).expect("read font"), 0).expect("load font");
    let mut text = TextEngine::new();
    text.add_font("sans", font);
    text
}

/// A root for the run's slots, removed first so a previous run cannot be the answer.
fn root() -> PathBuf {
    let root = std::env::temp_dir().join(format!("vela-test-slots-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    root
}

/// A run saves into its own slot, leaves the line, and loads the slot back.
///
/// The whole journey in one test, because the parts are only worth anything together: the slot has to be a
/// file (`slots` lists it, loadable, with a time the run stamped), and the story has to come back to the
/// command the slot was taken at — which is what `expect shown` after the load is for.
#[test]
fn a_slot_a_run_saves_is_a_file_it_comes_back_to() {
    // The whole journey through the *screen*, which is what the criterion asks for: an empty cell is pressed
    // to save (`1`, the number it draws), the story leaves the line, and the slot the screen now lists is
    // what is pressed to come back (`1-1`, the name it draws) — on the screen named `load`, which is what
    // tells the same action to load rather than to save.
    let directives = "    run from start\n    click \"Save\"\n    click \"1\"\n    click \"Back\"\n    \
                      advance 1\n    choose \"Right\"\n    expect shown \"Done.\"\n    \
                      click \"Load\"\n    click \"1-1\"\n    expect shown \"One.\"\n";
    let text = format!("{STORY}\ntest \"a test\":\n{directives}");
    let (module, plans) = suite(&text);
    let screens = screens();
    let mut text_engine = engine();
    let mut stage = Stage::new(&screens, &mut text_engine, "sans");
    assert!(stage.open("pause"), "the fixture declares `pause`");

    let root = root();
    let saves = Saves::new(&root, schema(&text));
    let report = run(&module, "start", &plans, Some(&mut stage), Some(&saves));
    let outcome = report.outcomes.into_iter().next().expect("one outcome");
    assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);

    // The slot is a real file, in the *test's* directory rather than the root, and it is one this build
    // can read: page one, slot one (`vela_replay::slot_name`).
    let mine = saves.of(0);
    let found = slots(mine.dir()).expect("the slots list");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].name, "1-1");
    assert!(found[0].loadable, "{:?}", found[0]);
    assert!(found[0].time > 0, "a run stamps the slot it writes");

    // And nothing was written outside it: the root holds one directory, which is this test's.
    let siblings: Vec<String> = std::fs::read_dir(&root)
        .expect("the root")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(siblings, vec!["0"], "one test, one directory");

    let _ = std::fs::remove_dir_all(&root);
}

/// Two tests in one run cannot see each other's slots.
///
/// The property that makes a run's slots usable in a suite: a test asserting "this page is empty" has to be
/// right whatever ran before it, and `Saves::of` is what keeps them apart.
#[test]
fn two_tests_do_not_share_their_slots() {
    let root = root();
    let saves = Saves::new(&root, schema(STORY));

    assert!(
        slots(saves.of(0).dir()).expect("list").is_empty(),
        "a test that has not saved has no slots"
    );
    assert_ne!(saves.of(0).dir(), saves.of(1).dir(), "two tests, two paths");

    let _ = std::fs::remove_dir_all(&root);
}
