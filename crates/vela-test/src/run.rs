//! Running a test against a compiled story.
//!
//! The runner is a step loop rather than `vela_vm::driver::run`, and that is the whole shape of this
//! file: `run` plays a story to the end, which is what a game does, where a test says "answer four
//! commands, then assert something" and then stops. `Session` is the step API — `advance`, `answer`,
//! `current` — so the loop below is the same twenty lines the driver is built from, with the script
//! answering instead of a host.

mod shown;

use vela_bytecode::Module;
use vela_vm::{Session, Step};
use vela_world::{Command, Input};

use crate::plan::{Plan, StepKind};
use crate::report::{Failure, Outcome, Reason, Report};
use crate::stage::Stage;
use shown::{assert, click, shown_assert, shown_text, shows, text};

/// Runs each plan against a compiled program.
///
/// `entry` is the label a test that does not say `run from` starts at, which is the project's own entry
/// point: a test with no `from` starts where the game does, because that is what "from the top" means.
///
/// `stage` is the screens a step may click and the ones the run has open (`crate::stage`); `None` is a
/// caller with none, and a click in such a run is a failure rather than a step that quietly does
/// nothing.
#[must_use]
pub fn run(
    module: &Module,
    entry: &str,
    plans: &[Plan],
    mut stage: Option<&mut Stage<'_>>,
) -> Report {
    Report {
        outcomes: plans
            .iter()
            .map(|plan| one(module, entry, plan, stage.as_deref_mut()))
            .collect(),
    }
}

/// Runs one test.
fn one(module: &Module, entry: &str, plan: &Plan, mut stage: Option<&mut Stage<'_>>) -> Outcome {
    let mut outcome = Outcome {
        name: plan.name.clone(),
        span: plan.span,
        failures: Vec::new(),
        notes: plan.notes.clone(),
    };

    let Some(mut session) = started(module, entry, plan, &mut outcome) else {
        return outcome;
    };

    let mut script = Script::default();
    drive(&mut session, plan, &mut script, &mut outcome, &mut stage);
    tail(
        module,
        &mut session,
        plan,
        &mut script,
        &mut outcome,
        &mut stage,
    );
    outcome
}

/// Starts the story a test runs from, or records that there is no such label.
fn started(module: &Module, entry: &str, plan: &Plan, outcome: &mut Outcome) -> Option<Session> {
    let start = plan
        .start
        .as_ref()
        .map_or_else(|| entry.to_string(), |path| path.join("."));
    let Ok(session) = Session::start(module, &start) else {
        outcome.failures.push(Failure::NoLabel {
            span: plan.start_span,
            label: start,
        });
        return None;
    };
    Some(session)
}

/// The rest of the script: assertions the story ended before reaching, and the coverage questions,
/// which are about the run that happened rather than about a position in it.
///
/// A run that already failed has nothing left worth saying about the steps it never reached: the reason
/// it stopped is the answer, and "the script has not finished" underneath it is the same fact told
/// twice, pointing at the same line.
fn tail(
    module: &Module,
    session: &mut Session,
    plan: &Plan,
    script: &mut Script,
    outcome: &mut Outcome,
    stage: &mut Option<&mut Stage<'_>>,
) {
    let failed = !outcome.failures.is_empty();
    while !failed && let Some(next) = plan.steps.get(script.index) {
        match &next.kind {
            StepKind::Expect { function, source } => {
                assert(session, function, source, next.span, outcome);
            }
            StepKind::ExpectShown {
                function,
                source,
                negated,
            } => {
                shown_assert(session, function, source, *negated, next.span, outcome);
            }
            StepKind::Cover(_) => cover(session, module, next.span, outcome),
            // A click in the tail is carried out like any other: a screen a test opened before the story
            // ended is still on the stack, and a press on it is still a press.
            StepKind::Click { function, source } => {
                if let Some(wanted) = text(session, function, source, next.span, outcome) {
                    click(stage, &wanted, next.span, outcome);
                }
            }
            // A wait the story never satisfied is why the run stopped, and saying *what it wanted*
            // is more use than "the story ended": the wait is the step that has something to report.
            StepKind::AdvanceUntil { function, source } => {
                if let Some(wanted) = text(session, function, source, next.span, outcome) {
                    outcome.failures.push(Failure::NotShown {
                        span: next.span,
                        wanted,
                        shown: shown_text(session),
                    });
                }
            }
            // A directive the story never reached is a script that wanted a screen that is not there:
            // the story ended first, and that is the report.
            StepKind::Advance(_) | StepKind::Choose { .. } => {
                // Only a story that is over owes an explanation; a script that stopped driving leaves
                // the story running, which is what a test of the first four lines does on purpose.
                if session.is_finished() {
                    outcome
                        .failures
                        .push(Failure::StoryEnded { span: next.span });
                }
                break;
            }
        }
        script.index += 1;
    }
}

/// Drives the story through the script's answers, until one of them runs out.
fn drive(
    session: &mut Session,
    plan: &Plan,
    script: &mut Script,
    outcome: &mut Outcome,
    stage: &mut Option<&mut Stage<'_>>,
) {
    let mut step = session.advance();

    loop {
        match step {
            Step::Fault(fault) => {
                outcome.failures.push(Failure::Fault {
                    span: plan.span,
                    message: fault.to_string(),
                });
                return;
            }
            // `Continue` is what `execute` says when there is more to do in the same step, and
            // `Session::advance` never hands it back: it runs to the next suspension or to the end.
            Step::Halt | Step::Continue => return,
            Step::Yield(_) => match answer(session, plan, script, outcome, stage) {
                Some(next) => step = next,
                None => return,
            },
        }
    }
}

/// Answers the command on screen, and returns the step that follows it.
///
/// `None` when the run stops here: the script has said everything it means to answer, or it has made a
/// mistake that has already been reported.
fn answer(
    session: &mut Session,
    plan: &Plan,
    script: &mut Script,
    outcome: &mut Outcome,
    stage: &mut Option<&mut Stage<'_>>,
) -> Option<Step> {
    // Assertions that sit before the answer to this command run here, because the world they are about
    // is the world as it is *at* this command — and they do not consume it, so the answer below is still
    // the answer to the command on screen.
    if !assertions(session, stage, plan, script, outcome) {
        return None;
    }

    // A `cover` is a claim about *every* label, so it is a claim about the whole run: the presence of
    // one below the cursor is what decides whether the story is driven to its end. Without one, a script
    // that has said everything it means to answer leaves the story where it is — a test is allowed to
    // check the first four lines and stop.
    let covering = plan.steps[script.index..]
        .iter()
        .any(|step| matches!(step.kind, StepKind::Cover(_)));

    let Some(next) = plan.steps.get(script.index) else {
        if !covering || !answer_plain(session, script, None, outcome) {
            return None;
        }
        return Some(session.answer(Input::Ack));
    };

    match &next.kind {
        StepKind::Advance(count) => {
            if script.remaining == 0 {
                script.remaining = *count;
            }
            if let Some(Command::Menu { choices, .. }) = session.current() {
                outcome.failures.push(Failure::UnscriptedChoice {
                    span: next.span,
                    offered: texts(choices),
                });
                return None;
            }
            script.remaining -= 1;
            if script.remaining == 0 {
                script.index += 1;
            }
            Some(session.answer(Input::Ack))
        }
        StepKind::Choose {
            function, source, ..
        } => choose(session, script, outcome, next.span, function, source),
        // A `cover` below the cursor drives the story on, and the loop above has already consumed every
        // assertion that precedes this command.
        StepKind::Cover(_) => {
            if !answer_plain(session, script, Some(next.span), outcome) {
                return None;
            }
            Some(session.answer(Input::Ack))
        }
        // A wait whose condition does not hold yet: answer, and look again at the next command.
        StepKind::AdvanceUntil { .. } => {
            if !answer_plain(session, script, Some(next.span), outcome) {
                return None;
            }
            Some(session.answer(Input::Ack))
        }
        // Consumed by the loop above, which stops at the first step that is not one.
        StepKind::Expect { .. } | StepKind::ExpectShown { .. } | StepKind::Click { .. } => None,
    }
}

/// Consumes every step that applies *now*, and says whether the run may go on.
///
/// The steps about the current state are all of them that fit: an `expect`, an `expect shown` and an
/// `advance until shown` whose subject already holds are claims about the command the story is waiting
/// on, and consuming them is what lets `advance until "x"` be followed by `choose "x"` as one
/// instruction rather than two.
///
/// A `click` is one of these rather than an answer, and that is the whole reason it is consumed *here*:
/// the screen is drawn over the story, and a player clicking a pause menu is not advancing the line
/// behind it. So `click "History"` then `click "Return"` then `advance 1` is one sequence — the two
/// presses, then one line — where a click answered as a command would make it three advances.
fn assertions(
    session: &mut Session,
    stage: &mut Option<&mut Stage<'_>>,
    plan: &Plan,
    script: &mut Script,
    outcome: &mut Outcome,
) -> bool {
    while let Some(next) = plan.steps.get(script.index) {
        match &next.kind {
            StepKind::Expect { function, source } => {
                assert(session, function, source, next.span, outcome);
                script.index += 1;
            }
            StepKind::ExpectShown {
                function,
                source,
                negated,
            } => {
                shown_assert(session, function, source, *negated, next.span, outcome);
                script.index += 1;
            }
            StepKind::Click { function, source } => {
                let Some(wanted) = text(session, function, source, next.span, outcome) else {
                    return false;
                };
                click(stage, &wanted, next.span, outcome);
                script.index += 1;
            }
            StepKind::AdvanceUntil { function, source } => {
                let Some(wanted) = text(session, function, source, next.span, outcome) else {
                    return false;
                };
                if !shows(session, &wanted) {
                    break;
                }
                script.index += 1;
            }
            _ => break,
        }
    }
    true
}

/// Where the script is.
#[derive(Default)]
struct Script {
    /// The next step to run.
    index: usize,
    /// How many commands the current `advance` still has to answer.
    remaining: u32,
    /// How many commands this test has answered, which is what bounds a story that does not end.
    answered: usize,
}

/// How many commands a `cover`-driven run may answer before it is a loop rather than a story.
const LIMIT: usize = 10_000;

/// Answers the next choice from the script.
///
/// A `choose` means *the next choice*, not the next command: a player hears the dialogue on the way to
/// it, and a script that had to count the lines first would be a script about the presentation rather
/// than about the story. `advance` is the directive that counts; this one looks for a question.
fn choose(
    session: &mut Session,
    script: &mut Script,
    outcome: &mut Outcome,
    span: vela_span::Span,
    function: &str,
    source: &str,
) -> Option<Step> {
    let wanted = text(session, function, source, span, outcome)?;

    // A `choose` means *the next choice*, not the next command: a player hears the dialogue on
    // the way to it, and a script that had to count the lines first would be a script about the
    // presentation rather than about the story. `advance` is the directive that counts.
    loop {
        if let Some(Command::Menu { choices, .. }) = session.current() {
            let Some(choice) = choices.iter().find(|choice| choice.text == wanted) else {
                outcome.failures.push(Failure::NoSuchChoice {
                    span,
                    wanted,
                    offered: texts(choices),
                });
                return None;
            };
            let answer = choice.index;
            script.index += 1;
            return Some(session.answer(Input::Choice(answer)));
        }

        if !answer_plain(session, script, Some(span), outcome) {
            return None;
        }
        let step = session.answer(Input::Ack);
        if !matches!(step, Step::Yield(_)) {
            outcome.failures.push(Failure::Uncarried {
                span,
                reason: Reason::Ended,
            });
            return None;
        }
    }
}

/// Answers a command that needs no decision: `Ack`, unless the story is asking a question.
///
/// Returns whether the run should continue.
fn answer_plain(
    session: &mut Session,
    script: &mut Script,
    span: Option<vela_span::Span>,
    outcome: &mut Outcome,
) -> bool {
    if let Some(Command::Menu { choices, .. }) = session.current() {
        outcome.failures.push(Failure::UnscriptedChoice {
            span: span.unwrap_or(outcome.span),
            offered: texts(choices),
        });
        return false;
    }
    script.answered += 1;
    if script.answered > LIMIT {
        outcome.failures.push(Failure::Runaway {
            span: span.unwrap_or(outcome.span),
            commands: script.answered,
        });
        return false;
    }
    true
}

/// Answers a `cover labels`: every label in the program was entered by this run.
fn cover(session: &Session, module: &Module, span: vela_span::Span, outcome: &mut Outcome) {
    let entered = session.entered_labels();
    let mut missed: Vec<String> = module
        .labels
        .iter()
        .map(|label| module.strings.get(label.name).unwrap_or("").to_string())
        .filter(|name| !entered.contains(name))
        .collect();
    missed.sort();

    if !missed.is_empty() {
        outcome.failures.push(Failure::Uncovered {
            span,
            labels: missed,
        });
    }
}

/// The texts a menu offers, in the order it offers them.
fn texts(choices: &[vela_world::Choice]) -> Vec<String> {
    choices.iter().map(|choice| choice.text.clone()).collect()
}
