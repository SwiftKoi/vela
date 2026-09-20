//! What a step reads out of the run, rather than what drives it.
//!
//! The step loop in `run` answers commands and decides what a script meant; this is everything a step
//! *says*: calling the function an expression was compiled into (`text`, `assert`), reading what the
//! story is showing a player (`shows`, `shown_text`, `screen_text`), the assertion about it, and the
//! click that presses what a screen offers (`click`). One child module because they are one subject —
//! the live run as a step finds it — and because none of them advances the story: a helper that did
//! would be the loop's, and the loop is the parent.

use vela_span::Span;
use vela_ui::Done;
use vela_vm::Session;
use vela_world::{Command, Value};

use crate::report::{Failure, Outcome, Reason};
use crate::saves::Saves;
use crate::stage::Stage;

use super::files;

/// Calls an assertion and reports it if it is not `true`.
pub(super) fn assert(
    session: &mut Session,
    name: &str,
    source: &str,
    span: Span,
    outcome: &mut Outcome,
) {
    match session.call(name) {
        Ok(Value::Bool(true)) => {}
        Ok(Value::Bool(false)) => outcome.failures.push(Failure::Assertion {
            span,
            source: source.to_string(),
        }),
        Ok(other) => outcome.failures.push(Failure::Uncarried {
            span,
            reason: Reason::Unreadable {
                message: format!("it produced {}", other.type_name()),
            },
        }),
        Err(fault) => outcome.failures.push(Failure::Uncarried {
            span,
            reason: Reason::Unreadable {
                message: fault.to_string(),
            },
        }),
    }
}

/// Asserts what is on screen, and says what is there when it is not what was asked for.
pub(super) fn shown_assert(
    session: &mut Session,
    name: &str,
    source: &str,
    negated: bool,
    span: Span,
    outcome: &mut Outcome,
) {
    let Some(wanted) = text(session, name, source, span, outcome) else {
        return;
    };
    if shows(session, &wanted) == negated {
        outcome.failures.push(Failure::Shown {
            span,
            wanted,
            negated,
            shown: shown_text(session),
        });
    }
}

/// Whether what is on screen contains this text.
///
/// Containment, case-insensitively — which is Ren'Py's rule, read out of `testfocus.find_focus` (`a
/// pattern in text`, both folded), and the reason a test may write `advance until "ask her right away"`
/// for the option a screen spells `Ask her right away.`. A test that had to reproduce the punctuation
/// of what it waits for would be a test about spelling.
pub(super) fn shows(session: &Session, wanted: &str) -> bool {
    let wanted = wanted.to_lowercase();
    screen_text(session)
        .iter()
        .any(|text| text.to_lowercase().contains(&wanted))
}

/// What is on screen, as one line a failure can print: `"We get married shortly after that."`.
pub(super) fn shown_text(session: &Session) -> String {
    let lines = screen_text(session);
    match lines.is_empty() {
        true => "nothing".to_string(),
        false => lines
            .iter()
            .map(|line| format!("`{line}`"))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// Everything the command on screen puts in front of a player.
///
/// The command the story is *waiting on*, which is what a player is looking at: a line's text, and a
/// menu's prompt and options. A screen drawn over it is not here — nothing in a headless run opens
/// one (`SCREENS.md §2.3`), so a claim about a screen is a claim this runner cannot check yet.
fn screen_text(session: &Session) -> Vec<String> {
    match session.current() {
        Some(Command::Say { text, .. }) => vec![text.clone()],
        Some(Command::Menu { prompt, choices }) => prompt
            .iter()
            .cloned()
            .chain(choices.iter().map(|choice| choice.text.clone()))
            .collect(),
        _ => Vec::new(),
    }
}

/// Calls an expression compiled into a function, and returns the text it produced.
///
/// What `choose`, `click`, `advance until shown` and `expect shown` all need and none of them owns:
/// the expression is the author's, the function is the compiler's, and reading it out of the live
/// story is this one call.
pub(super) fn text(
    session: &mut Session,
    name: &str,
    source: &str,
    span: Span,
    outcome: &mut Outcome,
) -> Option<String> {
    match session.call(name) {
        Ok(Value::Str(wanted)) => Some(wanted),
        Ok(other) => {
            outcome.failures.push(Failure::Uncarried {
                span,
                reason: Reason::Unreadable {
                    message: format!("`{source}` produced {}", other.type_name()),
                },
            });
            None
        }
        Err(fault) => {
            outcome.failures.push(Failure::Uncarried {
                span,
                reason: Reason::Unreadable {
                    message: fault.to_string(),
                },
            });
            None
        }
    }
}

/// Clicks the control whose words contain `text`, and carries out what it asks for.
///
/// A click is a player minus the window, so it goes through what a player's press goes through: the
/// control is *focused* — the words the test wrote are matched against what the control draws, which is
/// Ren'Py's rule for a text selector (`testfocus.find_focus`) — and the action is read from the focused
/// hotspot, so a click and a keypress cannot disagree about what a control does.
///
/// Nothing here answers the story. The story is waiting on a command and a screen is drawn over it, so
/// what a control does is the screen's business — which is why this is called where the *assertions*
/// are consumed (`run.rs`) rather than where the answers are.
pub(super) fn click(
    session: &mut Session,
    stage: &mut Option<&mut Stage<'_>>,
    saves: Option<&Saves>,
    wanted: &str,
    span: Span,
    outcome: &mut Outcome,
) {
    let Some(stage) = stage.as_deref_mut() else {
        outcome.failures.push(Failure::Uncarried {
            span,
            reason: Reason::NoScreens,
        });
        return;
    };

    // Nothing open is its own answer, not "the screen has no such control": what is on screen is the
    // question, and with nothing on the stack there was nothing to compare the words against. It is
    // also the state a test starts in — a screen is opened by a control or a key, and the first one has
    // to come from somewhere.
    if stage.is_empty() {
        outcome.failures.push(Failure::Uncarried {
            span,
            reason: Reason::NothingOpen,
        });
        return;
    }

    let Some(action) = stage.focus(wanted) else {
        outcome.failures.push(Failure::Uncarried {
            span,
            reason: Reason::NoSuchControl {
                wanted: wanted.to_string(),
                offered: stage.controls(),
            },
        });
        return;
    };

    // A setting is the *player's* state, and a run has a player (`RUNTIME.md §2.1`): the two setting
    // actions are carried out against the session's world, exactly as the windowed player carries them
    // out. So a test can press a settings button and the store changes, rather than the press being
    // refused as something a headless run cannot do — and the screens are laid out again, so what the next
    // click offers is what the screen says now.
    if vela_ui::settings::write(session.preferences_mut(), &action).is_some() {
        stage.set_preferences(session.world().preferences.clone());
        stage.relaid();
        return;
    }

    // A *file* action is carried out here too, against slots of the run's own (`vela_test::Saves`): item
    // 5's evidence is that a save screen works end to end, and a run that refused the press could not
    // produce it. What the action means is not decided here — the screen's name decides save-or-load, the
    // player's setting decides the page — so this is a join rather than a second implementation.
    if let Some(saves) = saves {
        let result = files::carry_out(session, saves, stage.top_name(), &action);
        if let Some(result) = result {
            if let Err(reason) = result {
                // The store said why: a slot that cannot be written or read is a real failure rather than
                // a headless run's limit, so it is reported in the store's own words.
                outcome.failures.push(Failure::Uncarried { span, reason });
            }
            return;
        }
    }

    // Everything the stack does not own needs the VM or the host, and a headless run has neither. Saying
    // so is the difference between a press that did nothing and a press that says it could not.
    if stage.carry_out(&action) == Done::NotOurs {
        outcome.failures.push(Failure::Uncarried {
            span,
            reason: Reason::NotOurs {
                action: action.to_string(),
                offered: stage.controls(),
            },
        });
    }
}
