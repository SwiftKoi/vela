//! What a run produced.
//!
//! Types only: the runner does not print, and the command line does. A failure carries the span of the
//! thing that failed — the directive in the test, or the test itself — so `vela test` renders it with
//! the same renderer every other diagnostic goes through, and the caret points at the author's line
//! rather than at a sentence about it.

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
    /// An assertion could not be read as a value.
    Unreadable {
        /// The directive's span.
        span: Span,
        /// Why not.
        message: String,
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
            | Self::Unreadable { span, .. }
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
            Self::Unreadable { message, .. } => {
                format!("the assertion could not be read: {message}")
            }
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

/// A list of option texts, for a message.
fn quoted(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|item| format!("`{item}`")).collect();
    if quoted.is_empty() {
        "nothing".to_string()
    } else {
        quoted.join(", ")
    }
}
