//! What a run produced.
//!
//! Types only: the runner does not print, and the command line does. A failure carries the span of the
//! thing that failed — the directive in the test, or the test itself — so `vela test` renders it with
//! the same renderer every other diagnostic goes through, and the caret points at the author's line
//! rather than at a sentence about it.

use std::fmt;

use vela_span::Span;

/// What running a suite produced.
#[derive(Debug)]
pub struct Report {
    /// One per test, in the order the files and the tests were read.
    pub outcomes: Vec<Outcome>,
}

impl Report {
    /// How many tests passed.
    #[must_use]
    pub fn passed(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|outcome| outcome.failures.is_empty())
            .count()
    }

    /// How many failed.
    #[must_use]
    pub fn failed(&self) -> usize {
        self.outcomes.len() - self.passed()
    }

    /// Whether the suite is green, which is what decides an exit code.
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.failed() == 0
    }
}

/// What one test did.
#[derive(Debug)]
pub struct Outcome {
    /// The test's name.
    pub name: String,
    /// The test's span.
    pub span: Span,
    /// Every way it failed. Empty means it passed.
    pub failures: Vec<Failure>,
    /// What the test asked for that this runner does not do.
    ///
    /// Reported rather than ignored: a directive that silently does nothing is a test that checks less
    /// than it says, which is the failure mode this whole crate exists to avoid.
    pub notes: Vec<String>,
}

impl Outcome {
    /// Whether this test passed.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.failures.is_empty()
    }
}

/// One way a test failed.
#[derive(Debug)]
pub enum Failure {
    /// The story faulted, which is a compiler bug rather than a test's mistake.
    Fault {
        /// Where to point.
        span: Span,
        /// What went wrong.
        message: String,
    },
    /// The test starts at a label the program does not have.
    NoLabel {
        /// The label's span in the test.
        span: Span,
        /// The label, fully qualified.
        label: String,
    },
    /// The story offered a choice, and the script did not say what to pick.
    UnscriptedChoice {
        /// The step that ran into it.
        span: Span,
        /// What was on offer.
        offered: Vec<String>,
    },
    /// A `choose` named an option the menu did not offer.
    NoSuchChoice {
        /// The directive's span.
        span: Span,
        /// What it asked for.
        wanted: String,
        /// What was on offer.
        offered: Vec<String>,
    },
    /// What a test waited for never appeared on screen.
    NotShown {
        /// The directive's span.
        span: Span,
        /// What it waited for.
        wanted: String,
        /// What it shows, as it was answered.
        shown: String,
    },
    /// An assertion about the screen was wrong.
    Shown {
        /// The directive's span.
        span: Span,
        /// What it looked for.
        wanted: String,
        /// Whether it looked for its absence.
        negated: bool,
        /// What is on screen.
        shown: String,
    },
    /// An assertion was false.
    Assertion {
        /// The directive's span.
        span: Span,
        /// The assertion, as the author wrote it.
        source: String,
    },
    /// An assertion could not be read as a value, or a step could not be carried out.
    ///
    /// One variant per *outcome* rather than one per reason: what a caller knows is what stopped it,
    /// and what this file knows is how that reads. So a step that reads nothing out of the live
    /// story, a `click` that names no control, and a control that asked for something a headless run
    /// does not carry out are one failure — the step did not happen — with three explanations.
    Uncarried {
        /// The step's span.
        span: Span,
        /// What stopped it.
        reason: Reason,
    },
    /// `cover labels` found labels the run never reached.
    Uncovered {
        /// The directive's span.
        span: Span,
        /// The labels, sorted, capped by the caller's patience rather than here.
        labels: Vec<String>,
    },
    /// The story presented more commands than a test is willing to answer.
    ///
    /// A bounded run rather than an unbounded one: a story with a cycle in it would otherwise hang CI,
    /// and "the story never ends" is a real thing for a test to discover.
    Runaway {
        /// The step that was driving the run.
        span: Span,
        /// How many commands it saw.
        commands: usize,
    },
    /// The script still had an answer to give when the story was over.
    StoryEnded {
        /// The step that was never reached.
        span: Span,
    },
}

impl Failure {
    /// Where the failure happened.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Fault { span, .. }
            | Self::NoLabel { span, .. }
            | Self::UnscriptedChoice { span, .. }
            | Self::NoSuchChoice { span, .. }
            | Self::NotShown { span, .. }
            | Self::Shown { span, .. }
            | Self::Assertion { span, .. }
            | Self::Uncarried { span, .. }
            | Self::Uncovered { span, .. }
            | Self::Runaway { span, .. }
            | Self::StoryEnded { span } => *span,
        }
    }

    /// What to say about it, in one line.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::Fault { message, .. } => format!("the story faulted: {message}"),
            Self::NoLabel { label, .. } => format!("there is no label `{label}` to start at"),
            Self::UnscriptedChoice { offered, .. } => {
                format!(
                    "the story offers a choice and the script does not say which: {}",
                    quoted(offered)
                )
            }
            Self::NoSuchChoice {
                wanted, offered, ..
            } => format!(
                "no option reads `{wanted}`; the story offers {}",
                quoted(offered)
            ),
            Self::NotShown { wanted, shown, .. } => {
                format!("`{wanted}` never appeared on screen; the story ended showing {shown}")
            }
            Self::Shown {
                wanted,
                negated,
                shown,
                ..
            } => match negated {
                true => format!("`{wanted}` is on screen, and the test says it is not: {shown}"),
                false => format!("`{wanted}` is not on screen, which shows {shown}"),
            },
            Self::Assertion { source, .. } => format!("`{source}` is not true"),
            Self::Uncarried { reason, .. } => reason.to_string(),
            Self::Uncovered { labels, .. } => {
                format!("the run never reached {}", quoted(labels))
            }
            Self::Runaway { commands, .. } => format!(
                "the story presented {commands} commands without ending, which is a loop rather than a \
                 story"
            ),
            Self::StoryEnded { .. } => {
                "the story is over, and the script has not finished".to_string()
            }
        }
    }
}

/// Why a step could not be carried out.
///
/// The sentence for each one is written here, once: a caller knows *what* happened, and this file is
/// the one that knows how a failure reads. So a reason cannot arrive without its wording, and two
/// sites that stop a step for the same reason cannot word it differently.
#[derive(Debug)]
pub enum Reason {
    /// The expression a step reads did not produce what the step needs — not a `bool` where an
    /// assertion wanted one, not text where a text was wanted, or a fault while evaluating it.
    Unreadable {
        /// What was wrong, in the words of the site that saw it.
        message: String,
    },
    /// The story ended before the step could be carried out.
    Ended,
    /// A `click` in a run with no screens at all: the caller loaded none, so there is nothing to press.
    NoScreens,
    /// A `click` with no screen open: there was nothing to match the words against.
    NothingOpen,
    /// A `click` found no control on the screen reading those words.
    NoSuchControl {
        /// What the test named.
        wanted: String,
        /// Every word the top screen's controls read.
        offered: Vec<String>,
    },
    /// A control asked for an action a headless run does not carry out: a `jump`, a save, a
    /// preference — everything that is the VM's or the host's rather than the screen stack's.
    NotOurs {
        /// The action, as the screen wrote it: `quit`, `jump(forest)`, …
        action: String,
        /// Every word the top screen's controls read.
        offered: Vec<String>,
    },
}

impl fmt::Display for Reason {
    /// The sentence a failure prints.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable { message } => write!(f, "the step could not be read: {message}"),
            Self::Ended => write!(f, "the story ended before this step could be carried out"),
            Self::NoScreens => write!(f, "this run has no screens, so nothing can be clicked"),
            Self::NothingOpen => write!(
                f,
                "no screen is open, so there is no control to click: what opens the first one is a \
                 key binding or the game's own menu (`SCREENS.md §2.3`)"
            ),
            Self::NoSuchControl { wanted, offered } => write!(
                f,
                "no control on the screen reads `{wanted}`; it offers {}",
                quoted(offered)
            ),
            Self::NotOurs { action, offered } => write!(
                f,
                "the control asked for `{action}`, which a headless run does not carry out; the \
                 screen offers {}",
                quoted(offered)
            ),
        }
    }
}

/// A list of option texts, for a message.
fn quoted(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|item| format!("`{item}`")).collect();
    if quoted.is_empty() {
        "nothing".to_string()
    } else {
        quoted.join(", ")
    }
}
