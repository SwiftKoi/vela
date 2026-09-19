//! What a step reads out of the run, rather than what drives it.
//!
//! The step loop in `run` answers commands and decides what a script meant; this is everything a step
//! *says*: calling the function an expression was compiled into (`text`, `assert`), reading what the
//! story is showing a player (`shows`, `shown_text`, `screen_text`), and the assertion about it. One
//! child module because they are one subject — the live run as a step finds it — and because none of
//! them advances the story: a helper that did would be the loop's, and the loop is the parent.

use vela_span::Span;
use vela_vm::Session;
use vela_world::{Command, Value};

use crate::report::{Failure, Outcome};

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
        Ok(other) => outcome.failures.push(Failure::Unreadable {
            span,
            message: format!("it produced {}", other.type_name()),
        }),
        Err(fault) => outcome.failures.push(Failure::Unreadable {
            span,
            message: fault.to_string(),
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
            outcome.failures.push(Failure::Unreadable {
                span,
                message: format!("`{source}` produced {}", other.type_name()),
            });
            None
        }
        Err(fault) => {
            outcome.failures.push(Failure::Unreadable {
                span,
                message: fault.to_string(),
            });
            None
        }
    }
}
