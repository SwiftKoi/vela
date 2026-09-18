//! Turning a file that declares tests into something runnable.
//!
//! # Why the expressions become functions
//!
//! A test's `expect` and `choose` hold *expressions* (`LANGUAGE.md §7.6`), and an expression has to be
//! evaluated by the VM: `visited(forest.river)` needs the world, and "what does this expression mean"
//! already has exactly one answer in this repository. So each one is compiled — a nullary function is
//! appended to the file, and the runner reads its value out of the live story with `Session::call`.
//!
//! Appending to the **text** rather than building syntax nodes by hand is the part worth explaining. It
//! means an assertion goes through the parser, the checker, lowering, the verifier, and the same
//! evaluation as everything else, and that the body of the synthesized function is the author's own
//! expression sliced out of the file by its span. Nothing re-prints an expression, so nothing can print
//! it differently from how it was written.

use vela_span::{FileId, Span};
use vela_syntax::{CoverMode, DirectiveKind, Item};

/// A file, with the functions a test's expressions need, and the tests themselves.
#[derive(Debug)]
pub struct Prepared {
    /// The text to parse and compile instead of the file's own.
    ///
    /// Identical to the original apart from the appended functions, which is what keeps every span in
    /// the file — including the ones a failure prints — pointing where it did.
    pub text: String,
    /// What to run.
    pub plans: Vec<Plan>,
}

/// One test, ready to run.
#[derive(Debug)]
pub struct Plan {
    /// The test's name, as written.
    pub name: String,
    /// The test's span, for a failure that is about the test as a whole.
    pub span: Span,
    /// The label `run from` names, as a dotted path. `None` means "wherever the project starts".
    pub start: Option<Vec<String>>,
    /// The start's own span, for a diagnostic about a label that cannot be resolved.
    pub start_span: Span,
    /// What to do, in order.
    pub steps: Vec<Step>,
    /// What this test asked for that reading it could not answer.
    pub notes: Vec<String>,
}

impl Plan {
    /// Qualifies the functions this plan calls with the module the test was read from.
    ///
    /// A *linked* program names everything `module.name` — labels and functions alike — because two
    /// modules may each declare `start` (`vela-mir::link`). A plan is read from one file and knows none of
    /// that, so the caller that linked the program is the one that says so: `vela test` calls this once
    /// per plan, and a single-module test — which is what this crate's own tests are — needs nothing.
    ///
    /// The start label is deliberately *not* touched here. It is not a name that gets prefixed, it is a
    /// reference that gets *resolved*: `run from chapters.forest.clearing` starts in another module, and
    /// only the caller that has every module's aliases can say where that is.
    pub fn qualify(&mut self, module: &str) {
        for step in &mut self.steps {
            let function = match &mut step.kind {
                StepKind::Choose { function, .. }
                | StepKind::Expect { function, .. }
                | StepKind::AdvanceUntil { function, .. }
                | StepKind::ExpectShown { function, .. } => function,
                StepKind::Advance(_) | StepKind::Cover(_) => continue,
            };
            *function = format!("{module}.{function}");
        }
    }
}

/// One step of a test.
#[derive(Debug)]
pub struct Step {
    /// The directive's span, which is what a failure points at.
    pub span: Span,
    /// What it says to do.
    pub kind: StepKind,
}

/// What a step says to do.
#[derive(Debug)]
pub enum StepKind {
    /// Answer this many commands that are not choices.
    Advance(u32),
    /// Answer the next choice with the option whose text this function produces.
    Choose {
        /// The synthesized function that produces the text.
        function: String,
        /// The expression as the author wrote it, for a failure message.
        source: String,
    },
    /// Advance until the text this function produces is on screen.
    AdvanceUntil {
        /// The synthesized function that produces the text.
        function: String,
        /// The expression as the author wrote it, for a failure message.
        source: String,
    },
    /// Assert that this function's text is on screen — or is not, when negated.
    ExpectShown {
        /// The synthesized function that produces the text.
        function: String,
        /// The expression as the author wrote it, for a failure message.
        source: String,
        /// Whether the assertion is that it is *not* there.
        negated: bool,
    },
    /// Assert that this function produces `true`.
    Expect {
        /// The synthesized function that produces the value.
        function: String,
        /// The assertion as the author wrote it, for a failure message.
        source: String,
    },
    /// Assert something about the run as a whole.
    Cover(CoverMode),
}

/// Reads a file's tests, and appends the functions their expressions need.
///
/// A file with no tests is returned unchanged, with no plans: most files are stories.
#[must_use]
pub fn prepare(file: FileId, text: &str) -> Prepared {
    let parsed = vela_syntax::parse(file, text);
    let mut plans = Vec::new();
    let mut appended = String::new();
    let mut counter = 0u32;

    for item in &parsed.program.items {
        let Item::Test(decl) = item else {
            continue;
        };
        plans.push(plan(decl, text, &mut appended, &mut counter));
    }

    let mut prepared = text.to_string();
    prepared.push_str(&appended);
    Prepared {
        text: prepared,
        plans,
    }
}

/// One test's plan, appending what its expressions need.
fn plan(
    decl: &vela_syntax::TestDecl,
    text: &str,
    appended: &mut String,
    counter: &mut u32,
) -> Plan {
    let mut plan = Plan {
        name: decl.name.clone(),
        span: decl.span,
        start: None,
        start_span: decl.span,
        steps: Vec::new(),
        notes: Vec::new(),
    };

    for directive in &decl.directives {
        step(directive, text, appended, counter, &mut plan);
    }

    plan
}

/// One directive, as a step — or as a note about why it is not one.
fn step(
    directive: &vela_syntax::Directive,
    text: &str,
    appended: &mut String,
    counter: &mut u32,
    plan: &mut Plan,
) {
    let span = directive.span;

    let kind = match &directive.kind {
        DirectiveKind::Run { target, .. } if target.is_empty() => return,
        DirectiveKind::Run { .. } => {
            start_of(&directive.kind, plan);
            return;
        }
        DirectiveKind::Advance { count } => {
            // `advance 0` would answer nothing and then read as a script that has run out, so it is a
            // typo rather than a step. Saying so beats a test that passes by doing nothing.
            if *count == 0 {
                plan.notes
                    .push("`advance 0` answers nothing; write a number above zero".to_string());
                return;
            }
            StepKind::Advance(*count)
        }
        // The two `shown` directives are one question asked twice — *is this on screen now*, and
        // *run on until it is* — so they are read once, in `shown`.
        DirectiveKind::AdvanceUntil { text: expr } => {
            match shown(expr, text, appended, counter, None) {
                Some(step) => step,
                None => {
                    plan.notes
                        .push("this `advance until` could not be read".to_string());
                    return;
                }
            }
        }
        DirectiveKind::ExpectShown {
            text: expr,
            negated,
        } => match shown(expr, text, appended, counter, Some(*negated)) {
            Some(step) => step,
            None => {
                plan.notes
                    .push("this `expect shown` could not be read".to_string());
                return;
            }
        },
        DirectiveKind::Choose { text: expr } => {
            let Some(source) = source_of(text, expr.span()) else {
                plan.notes
                    .push("this `choose` could not be read".to_string());
                return;
            };
            StepKind::Choose {
                function: function(appended, counter, "choose", "str", source),
                source: one_line(source),
            }
        }
        DirectiveKind::Expect { expr } => {
            let Some(source) = source_of(text, expr.span()) else {
                plan.notes
                    .push("this `expect` could not be read".to_string());
                return;
            };
            StepKind::Expect {
                function: function(appended, counter, "expect", "bool", source),
                source: one_line(source),
            }
        }
        DirectiveKind::Cover { mode } => match cover_mode(*mode, plan) {
            Some(step) => step,
            None => return,
        },
    };

    plan.steps.push(Step { span, kind });
}

/// A `cover` step, or a note saying why there is none.
///
/// `cover variants` is named and unimplemented: nothing records which enum variants a run matched, and
/// a test that silently checked nothing would be worse than one that says it cannot.
fn cover_mode(mode: CoverMode, plan: &mut Plan) -> Option<StepKind> {
    if mode == CoverMode::Variants {
        plan.notes.push(
            "`cover variants` is not implemented: nothing records which enum variants a run \
             matched"
                .to_string(),
        );
        return None;
    }
    Some(StepKind::Cover(mode))
}

/// Records where a `run from` starts, which is the one directive that is not a step.
fn start_of(kind: &vela_syntax::DirectiveKind, plan: &mut Plan) {
    let vela_syntax::DirectiveKind::Run {
        target,
        target_span,
    } = kind
    else {
        return;
    };
    plan.start = Some(target.clone());
    plan.start_span = *target_span;
}

/// A step whose subject is what is *on screen*, or `None` when the expression could not be read.
///
/// `negated` is `None` for a wait and `Some(false)`/`Some(true)` for an assertion, which is the only
/// difference between the two directives: one runs the story on until the subject holds, the other
/// asks whether it does.
fn shown(
    expr: &vela_syntax::Expr,
    text: &str,
    appended: &mut String,
    counter: &mut u32,
    negated: Option<bool>,
) -> Option<StepKind> {
    let source = source_of(text, expr.span())?;
    let function = function(appended, counter, "shown", "str", source);
    let source = one_line(source);
    match negated {
        Some(negated) => Some(StepKind::ExpectShown {
            function,
            source,
            negated,
        }),
        None => Some(StepKind::AdvanceUntil { function, source }),
    }
}

/// The source text of a span inside the file it came from.
fn source_of(text: &str, span: Span) -> Option<&str> {
    text.get(span.start() as usize..span.end() as usize)
}

/// Appends a nullary function that returns an expression, and answers its name.
fn function(
    appended: &mut String,
    counter: &mut u32,
    what: &str,
    ret: &str,
    source: &str,
) -> String {
    let name = format!("__vela_{what}_{counter}");
    *counter += 1;

    appended.push_str(&format!(
        "\nfn {name}() -> {ret}:\n    return {}\n",
        one_line(source)
    ));

    name
}

/// An expression on one line, whatever the author wrote.
///
/// A continuation line inside the synthesized `return` would carry its own indentation, and indentation
/// is structure in this language: `return a ==\n    b` is not `return a == b`.
fn one_line(source: &str) -> String {
    source.split_whitespace().collect::<Vec<_>>().join(" ")
}
