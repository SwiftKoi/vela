//! `testcases.rpy` → `test` items (`TOOLING.md §5`).
//!
//! A Ren'Py testcase drives the *interface*: it clicks a control by its text, waits for the picture to
//! say something, asserts what is on screen. Vela's test language is about the story — `run from`,
//! `advance <n>`, `choose <text>`, `expect <expr>` against the world — so this pass translates the
//! steps that name a *story* thing and reports the ones that name a screen, rather than writing a step
//! the runner cannot run (`docs/roadmap/M12.1-screen-language.md`, item 18).
//!
//! # The one judgement call, and why it is a rule rather than a guess
//!
//! `click "ask her later"` and `click "Start"` are the same instruction to Ren'Py — find the focus whose
//! text contains the pattern, and activate it — and different things to Vela: the first is a *menu
//! option*, which `choose <text>` already is, and the second is a screen control, which Vela has no step
//! for. The migrator can tell them apart from the *project* rather than by guessing: a click whose text
//! is contained in exactly one menu option in the story is that option (`choose "Ask her later."`), and
//! anything else is reported. Containment, case-insensitively, is Ren'Py's own rule for a text selector
//! (`renpy/test/testfocus.py`), which is why the sample may write `"ask her later"` for a screen that
//! shows `Ask her later.` — and "exactly one" is what keeps an ambiguous click out of the translation.

use std::collections::BTreeSet;

use crate::report::Report;
use crate::rpy::{Kind, Node};

/// What a `testcases.rpy` became.
pub struct Translated {
    /// The `test` items, as a Vela source file.
    pub text: String,
}

/// Translates a file of `testsuite`/`testcase` blocks, or `None` when it holds neither.
///
/// `menus` is every option text the project's stories offer. It is what turns a `click` into a
/// `choose`, and a project whose stories are not in this migration — a bundle, say — simply reports
/// every click instead.
pub fn lower(
    relative: &str,
    nodes: &[Node],
    menus: &BTreeSet<String>,
    report: &mut Report,
) -> Option<Translated> {
    let tests: Vec<&Node> = nodes
        .iter()
        .filter(|node| head(&node.text) == "testcase")
        .collect();
    let suites: Vec<&Node> = nodes
        .iter()
        .filter(|node| head(&node.text) == "testsuite")
        .collect();
    if tests.is_empty() && suites.is_empty() {
        return None;
    }

    let mut out = String::from(
        "# The project's testcases, migrated from Ren'Py by `vela migrate`. A step that names a\n\
         # *screen* — a click on a control, a wait for a screen name, a keypress — has no Vela\n\
         # equivalent yet, and `MIGRATION.md` lists each one.\n",
    );
    let mut written = 0usize;

    for suite in &suites {
        // A suite is a setup/teardown pair around every testcase, and Vela's `test` is one flat list
        // of directives: there is nowhere to put `before testcase:` and nothing that would run it.
        report.push(
            relative,
            suite.line,
            &suite.text,
            "a `testsuite` is a setup and teardown around every testcase in it, and a Vela `test` is \
             one flat list of steps — port the hooks by hand, or leave them out and let each test \
             start where it needs to",
        );
    }

    for test in &tests {
        if let Some(item) = test_item(test, relative, menus, report) {
            out.push_str(&item);
            written += 1;
        }
    }

    if written == 0 {
        report.push(
            relative,
            1,
            &format!("{relative} ({} testcase(s))", tests.len()),
            "every step of every testcase here names the interface, so there is no `test` item to              write: the entries above are the steps, and each is ported by hand (`TOOLING.md §5`)",
        );
        return None;
    }
    Some(Translated { text: out })
}

/// One `testcase` as a `test` item — or `None` when any of its steps is about the screen.
///
/// **All of it or none of it**, which is the one judgement this pass makes about a whole test. A test
/// is a *sequence*: `advance until …` then `expect …` asserts something about the state the wait left
/// behind, so dropping the wait does not leave a smaller test — it leaves a different one, which would
/// pass or fail for reasons that have nothing to do with the story. A testcase that runs to the wrong
/// conclusion is worse than one that was never written, so its steps are reported and it is not
/// emitted; a testcase whose every step is about the story is emitted whole.
fn test_item(
    test: &Node,
    relative: &str,
    menus: &BTreeSet<String>,
    report: &mut Report,
) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut missing = 0usize;

    for node in &test.children {
        let text = node.text.trim();
        if text.is_empty() || text.starts_with('#') {
            continue;
        }
        match step(text, menus) {
            Reading::Directive(line) => lines.push(line),
            Reading::Screen => {
                missing += 1;
                report.push(
                    relative,
                    node.line,
                    text,
                    "this step names the *interface* — a screen control, a screen name, a keypress or \
                     a wall-clock wait — and Vela's test language has no step for one yet, so the \
                     testcase around it is not translated either. What it checks is ported by hand \
                     (`TOOLING.md §5`)",
                );
            }
            Reading::Unreadable(reason) => {
                missing += 1;
                report.push(
                    relative,
                    node.line,
                    text,
                    format!(
                        "this step is not read: {reason}. The testcase around it is not \
                             translated either"
                    ),
                );
            }
        }
    }

    if missing > 0 || lines.is_empty() {
        report.push(
            relative,
            test.line,
            &format!("{} ({} steps)", test.text, lines.len() + missing),
            "this testcase is not translated: a test is a sequence, and one with a step missing is a \
             different test — it would run to a different conclusion and pass or fail for reasons \
             that have nothing to do with the story",
        );
        return None;
    }

    let mut out = format!("test {:?}:\n", name_of(&test.text));
    for line in &lines {
        out.push_str(&format!("    {line}\n"));
    }
    Some(out)
}

/// What one Ren'Py step became, or why it did not become anything.
enum Reading {
    /// A Vela directive.
    Directive(String),
    /// A step about the *interface*: a screen control, a screen name, a keypress, a wall-clock wait.
    /// Vela's test language has no step for one yet, and the entry says so.
    Screen,
    /// A step this pass does not read at all, with the reason to print.
    ///
    /// Kept apart from [`Reading::Screen`] because the two are different work: a screen step needs a
    /// step the language has not got, and an unreadable one usually needs a person to look at the
    /// line — a condition, a trailing comment, or Ren'Py's own runner configuration.
    Unreadable(&'static str),
}

/// One Ren'Py test step, read.
fn step(text: &str, menus: &BTreeSet<String>) -> Reading {
    if let Some(rest) = text.strip_prefix("click ") {
        // A click is a *menu option* when the project's own menus offer one that contains the text,
        // and a screen control otherwise — which is the difference between a step Vela has and one it
        // does not.
        return match unquote(rest) {
            Some(wanted) => match exactly_one(menus, &wanted) {
                Some(option) => Reading::Directive(format!("choose {option:?}")),
                None => Reading::Screen,
            },
            None => Reading::Unreadable("this `click` is not a quoted text"),
        };
    }
    if let Some(rest) = text.strip_prefix("advance until ") {
        return match unquote(rest) {
            Some(wanted) => Reading::Directive(format!("advance until shown {wanted:?}")),
            // A condition — `screen "choice"`, or `("a" or "b")` — is Ren'Py's own condition
            // language, and it is what a wait is for there.
            None => Reading::Screen,
        };
    }
    if let Some(rest) = text.strip_prefix("assert not ") {
        return match unquote(rest) {
            Some(wanted) => Reading::Directive(format!("expect not shown {wanted:?}")),
            None => {
                Reading::Unreadable("this `assert` is not a quoted text, or has something after it")
            }
        };
    }
    if let Some(rest) = text.strip_prefix("assert ") {
        return match unquote(rest) {
            Some(wanted) => Reading::Directive(format!("expect shown {wanted:?}")),
            None => {
                Reading::Unreadable("this `assert` is not a quoted text, or has something after it")
            }
        };
    }
    let reason = match head(text) {
        "keysym" => {
            "a keypress: Vela's input is a semantic action rather than a keycode \
                     (`SCREENS.md §11`)"
        }
        "pause" => "a wall-clock wait, and a Vela test has no clock: it is deterministic",
        "$" => "a Python statement, which is Ren'Py's runner configuring itself",
        "if" => "a conditional inside a test, which Vela's `test` has no shape for",
        "exit" => "`exit`, which is what a Vela test does when its lines run out",
        "run" => {
            "an action the test runs (`run MainMenu(confirm=False)` is Ren'Py's main menu, \
                  which arrives with M12.2)"
        }
        _ => "a step this pass does not read",
    };
    Reading::Unreadable(reason)
}

/// The one option text that contains `wanted`, when there is exactly one.
///
/// Containment and casefolding because that is how Ren'Py resolves a text selector, and *exactly one*
/// because a click that could mean two options is a click no rule can resolve — a translation that
/// picked one would be a test that passes for the wrong reason.
fn exactly_one(menus: &BTreeSet<String>, wanted: &str) -> Option<String> {
    let wanted = wanted.to_lowercase();
    let mut found = menus
        .iter()
        .filter(|option| option.to_lowercase().contains(&wanted));
    let first = found.next()?;
    found.next().is_none().then(|| first.clone())
}

/// A whole-line string literal, unquoted.
fn unquote(text: &str) -> Option<String> {
    let text = text.trim().trim_end_matches(':').trim();
    let text = text
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            text.strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
        })?;
    Some(text.to_string())
}

/// A `testcase`'s name: `testcase good_end:` is `good end`.
fn name_of(text: &str) -> String {
    let rest = text.trim().trim_start_matches("testcase").trim();
    rest.trim_end_matches(':').trim().to_string()
}

/// The first word of a line.
fn head(text: &str) -> &str {
    text.split_whitespace().next().unwrap_or("")
}

/// Every menu option text in a story, which is what a `click` is resolved against.
pub(crate) fn menu_options(nodes: &[Node]) -> BTreeSet<String> {
    let mut options = BTreeSet::new();
    let mut queue: std::collections::VecDeque<&Node> = nodes.iter().collect();
    while let Some(node) = queue.pop_front() {
        queue.extend(node.children.iter());
        let Kind::Choice { text, .. } = &node.kind else {
            continue;
        };
        if let Some(text) = unquote(text) {
            options.insert(text);
        }
    }
    options
}
