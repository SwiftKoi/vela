//! Clicking a control: what a press does, and what it says when it cannot.
//!
//! A click is a player minus the window, so these assert both halves of that: the control is found the
//! way Ren'Py finds one — the words it *draws*, casefolded — and what it asks for is carried out by the
//! same stack a window drives. The stage is set up here, because what opens a screen is not the test
//! language's yet (`TOOLING.md §5`), and a click with nothing open is one of the things asserted.

use vela_span::FileId;
use vela_text::{Font, TextEngine};
use vela_ui::ScreenSet;

use crate::report::{Failure, Reason};
use crate::run::run;
use crate::stage::Stage;

use super::support::{suite, with_test};

/// The screens a click presses: a pause menu, the screen one of its buttons opens, one button that asks
/// for something a headless run has no answer for, and one that changes one of the player's settings.
const SCREENS: &str = "\
screen pause:
    column gap 12:
        button:
            text \"Settings\"
            action open_screen(settings)
        button:
            text \"Return\"
            action close_screen()
        button:
            text \"Quit\"
            action quit()
        button:
            text \"Skip\"
            action toggle_preference(\"skip_unseen\")

screen settings:
    button:
        text \"Back\"
        action close_screen()
";

/// A text engine with the bundled face, because laying a screen out needs one to size text with.
fn engine() -> TextEngine {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fonts/LiberationSans-Regular.ttf");
    let font = Font::from_bytes(std::fs::read(path).expect("read font"), 0).expect("load font");
    let mut text = TextEngine::new();
    text.add_font("sans", font);
    text
}

/// The fixture's screens, compiled.
fn screens() -> ScreenSet {
    let parsed = vela_syntax::parse(FileId::from_raw(0), SCREENS);
    assert!(
        parsed.diagnostics.is_empty(),
        "the click fixture must parse: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.clone())
            .collect::<Vec<_>>()
    );
    ScreenSet::from_items(&parsed.program.items)
}

/// Runs a click test against the fixture's screens, with `pause` open — which is what a player would
/// have done with Escape, and what a test does by saying so.
fn clicked(directives: &str) -> (Vec<Failure>, Vec<String>) {
    let (module, plans) = suite(&with_test(directives));
    let screens = [screens()];
    let mut text = engine();
    let mut stage = Stage::new(&screens, &mut text, "sans");
    assert!(stage.open("pause"), "the fixture declares `pause`");

    let report = run(&module, "start", &plans, Some(&mut stage), None);
    let outcome = report.outcomes.into_iter().next().expect("one outcome");
    (outcome.failures, stage.controls())
}

/// A click carries out what the control asks for: `open_screen` lays the screen out and pushes it, so
/// what is on top after the press is the screen the button named.
#[test]
fn a_click_carries_out_what_the_control_asks_for() {
    let (failures, controls) = clicked("    run from start\n    click \"Settings\"\n");

    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(
        controls,
        ["Back"],
        "the screen the control opened is on top"
    );
}

/// A click does not answer the story. The line the story is waiting on is still waiting after two
/// presses, so two clicks and an advance are one sequence — not three advances, which is what a click
/// consumed where the *answers* are would make of them.
#[test]
fn a_click_does_not_answer_the_command_behind_it() {
    let (failures, controls) = clicked(
        "    run from start\n    click \"Settings\"\n    click \"Back\"\n    advance 1\n    \
         expect shown \"Which way?\"\n",
    );

    // The two presses: one opened `settings`, the other closed it, so `pause` is what is on top.
    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(
        controls,
        ["Settings", "Return", "Quit", "Skip"],
        "back on the pause menu"
    );
}

/// A click that names no control says what the screen *does* offer — the same shape as a `choose`
/// that names no option, and the only message a reader can act on.
#[test]
fn a_click_that_names_nothing_offers_the_words_that_were_there() {
    let (failures, _) = clicked("    run from start\n    click \"Nowhere\"\n");

    assert_eq!(failures.len(), 1, "{failures:?}");
    let Failure::Uncarried {
        reason: Reason::NoSuchControl { wanted, offered },
        ..
    } = &failures[0]
    else {
        panic!("expected a missing control, got {failures:?}");
    };
    assert_eq!(wanted, "Nowhere");
    assert_eq!(
        offered,
        &vec![
            "Settings".to_string(),
            "Return".to_string(),
            "Quit".to_string(),
            "Skip".to_string()
        ]
    );
    assert!(
        failures[0].message().contains("`Settings`"),
        "{}",
        failures[0].message()
    );
}

/// A control that asks for something the stack does not do — a `quit`, a `jump`, a save — fails, and
/// names the action rather than pretending the press did something.
#[test]
fn a_click_that_asks_for_the_vm_is_reported_as_such() {
    let (failures, _) = clicked("    run from start\n    click \"Quit\"\n");

    assert_eq!(failures.len(), 1, "{failures:?}");
    let Failure::Uncarried {
        reason: Reason::NotOurs { action, offered },
        ..
    } = &failures[0]
    else {
        panic!("expected a refused action, got {failures:?}");
    };
    assert_eq!(action, "quit()", "the action as the screen wrote it");
    assert_eq!(offered.len(), 4);
    assert!(
        failures[0]
            .message()
            .contains("headless run does not carry out"),
        "{}",
        failures[0].message()
    );
}

/// A click that changes one of the player's settings is carried out, not refused: a setting is the
/// *player's* state and a run has a player (`RUNTIME.md §2.1`), so a test can drive a settings screen the
/// same way a person does — which is the difference between a settings button a test can press and one
/// that reports that a headless run cannot.
#[test]
fn a_click_that_changes_a_setting_is_carried_out() {
    let (failures, _) = clicked("    run from start\n    click \"Skip\"\n");

    assert!(failures.is_empty(), "{failures:?}");
}

/// A screen draws what a setting says, and a press that changes one is what the *next* press reads.
///
/// The whole read path in one test: `setting("…")` answers the store the run injected, the words a control
/// draws come from it, and a press that flips the setting is followed by a re-lay — so what is on offer
/// changes with the store rather than staying what it was before the press (`SCREENS.md §7.1`). The
/// sequence *is* the assertion: the first click can only find `Skip: off` if the screen drew the
/// declaration, and the second can only find `Skip: on` if the press reached the store *and* the screen was
/// laid out again.
#[test]
fn a_screen_draws_a_setting_and_a_press_changes_what_it_draws() {
    const FLAGS: &str = "\
screen flags:
    if setting(\"skip_unseen\"):
        button:
            text \"Skip: on\"
            action toggle_preference(\"skip_unseen\")
    else:
        button:
            text \"Skip: off\"
            action toggle_preference(\"skip_unseen\")
";
    let (module, plans) = suite(&with_test(
        "    run from start\n    click \"Skip: off\"\n    click \"Skip: on\"\n",
    ));
    let parsed = vela_syntax::parse(FileId::from_raw(0), FLAGS);
    assert!(
        parsed.diagnostics.is_empty(),
        "the fixture must parse: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.clone())
            .collect::<Vec<_>>()
    );
    let screens = [ScreenSet::from_items(&parsed.program.items)];
    let mut text = engine();
    let mut stage = Stage::new(&screens, &mut text, "sans");
    assert!(stage.open("flags"), "the fixture declares `flags`");

    let report = run(&module, "start", &plans, Some(&mut stage), None);
    let outcome = report.outcomes.into_iter().next().expect("one outcome");
    assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
    // Two presses, and the label is back where it started: the second press flipped the setting back.
    assert_eq!(stage.controls(), ["Skip: off"]);
}

/// A click with no screens at all is a failure rather than a step that quietly does nothing — the
/// caller that wants a click has to bring a stage.
#[test]
fn a_click_without_a_stage_says_so() {
    let (module, plans) = suite(&with_test("    run from start\n    click \"Settings\"\n"));
    let report = run(&module, "start", &plans, None, None);

    assert_eq!(report.failed(), 1);
    let Failure::Uncarried {
        reason: Reason::NoScreens,
        ..
    } = &report.outcomes[0].failures[0]
    else {
        panic!(
            "expected a missing stage, got {:?}",
            report.outcomes[0].failures
        );
    };
}

/// And a click with screens but nothing open says *that* rather than blaming the control's name: what
/// is on screen is the question, and the two mistakes want different fixes.
#[test]
fn a_click_with_nothing_open_says_what_is_missing() {
    let (module, plans) = suite(&with_test("    run from start\n    click \"Settings\"\n"));
    let screens = [screens()];
    let mut text = engine();
    let mut stage = Stage::new(&screens, &mut text, "sans");
    let report = run(&module, "start", &plans, Some(&mut stage), None);

    assert_eq!(report.failed(), 1);
    let Failure::Uncarried {
        reason: Reason::NothingOpen,
        ..
    } = &report.outcomes[0].failures[0]
    else {
        panic!(
            "expected nothing open, got {:?}",
            report.outcomes[0].failures
        );
    };
    assert!(
        report.outcomes[0].failures[0]
            .message()
            .contains("no screen is open")
    );
}
