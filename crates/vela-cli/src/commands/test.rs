//! `vela test` — the headless story runner, and the accessibility sweep.
//!
//! Two subjects, one command. A project's `test` items (`TOOLING.md §5`) are its suite: scripted input,
//! world assertions, coverage of the labels a run reaches, and — since a test may click — the screens
//! the run has open, laid out against the project's own compiled sets. `--a11y` adds the M7 sweep:
//! every screen's focus order, failing on a focusable node with nothing to announce, which is `W4010`
//! enforced as a gate.
//!
//! The loading is this command's, not `vela-test`'s: the crate is handed a compiled program and the
//! tests read out of the session's files, and it never looks at a directory. That is the same division
//! the language server makes, and for the same reason — the project's files are the command line's
//! question, and a runner that answered it differently would be a runner that runs a different program.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use vela_compile::Session;
use vela_diag::Severity;
use vela_span::{FileId, Span};
use vela_syntax::{Item, ScreenDecl, parse};
use vela_test::{Plan, Report, Stage};
use vela_text::{Font, TextEngine};
use vela_ui::WidgetRegistry;
use vela_ui::a11y::A11yNode;

use crate::command::{Command, Error};
use crate::commands::check::{Project, collect, load};
use crate::commands::frame::{DEFAULT_FACE, FACE_NAME};
use crate::commands::resolve::{self, Table};
use crate::commands::run::flag_value;
use crate::commands::ui::Screens;

/// The `vela test` command.
pub struct Test {
    base: PathBuf,
}

impl Test {
    /// Creates the command rooted at `base`.
    #[must_use]
    pub fn new(base: impl Into<PathBuf>) -> Self {
        Self { base: base.into() }
    }

    /// Creates the command rooted at the process's working directory.
    #[must_use]
    pub fn at_current_dir() -> Self {
        let base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self::new(base)
    }
}

impl Command for Test {
    fn name(&self) -> &'static str {
        "test"
    }

    fn about(&self) -> &'static str {
        "run a project's tests and accessibility sweep"
    }

    fn run(&self, args: &[String], out: &mut dyn Write) -> Result<(), Error> {
        for unsupported in ["--update", "--seed"] {
            if args.iter().any(|arg| arg == unsupported) {
                return Err(Error::usage(format!(
                    "`{unsupported}` is for golden frames, which are not implemented yet"
                )));
            }
        }

        let target =
            positional(args).map_or_else(|| self.base.clone(), |path| self.base.join(path));
        let project = collect(&target)?;

        // The suite first, then the sweep: they are different subjects, and a reader wants two verdicts
        // rather than one list with both kinds of failure in it.
        tests(&project, args, out)?;
        if args.iter().any(|arg| arg == "--a11y") {
            sweep(&project, out)?;
        }
        Ok(())
    }
}

/// Runs every `test` item in the project.
fn tests(project: &Project, args: &[String], out: &mut dyn Write) -> Result<(), Error> {
    let filter = flag_value(args, "--filter").map(ToString::to_string);
    let mut session = load(project)?;

    let plans = prepare(&mut session, filter.as_deref());
    if plans.is_empty() {
        // Either the project has no tests yet — most do not — or the filter matched none of them, and
        // saying which is the difference between "nothing to run" and "you spelled it wrong".
        let _ = writeln!(
            out,
            "no tests{}",
            filter
                .as_ref()
                .map_or(String::new(), |wanted| format!(" matching `{wanted}`"))
        );
        return Ok(());
    }
    if session
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.severity() == Severity::Error)
    {
        return Err(Error::diagnostics(
            "the project has errors; run `vela check` to see them".to_string(),
        ));
    }

    let entry = project
        .manifest
        .as_ref()
        .map(|manifest| manifest.project.entry.clone())
        .ok_or_else(|| Error::usage("no entry point: set `entry` in vela.toml".to_string()))?;

    let linked = session
        .linked()
        .map_err(|error| Error::diagnostics(error.to_string()))?;
    let module = vela_bytecode::compile(&linked, true);
    let diagnostics = vela_bytecode::verify(&module);
    if !diagnostics.is_empty() {
        return Err(Error::internal(
            "the compiled module does not verify".to_string(),
        ));
    }

    // The screens a step may click, and the face to lay them out with: the same compiled sets and the
    // same bundled font `vela run` uses, so a test presses the control a player would press and the
    // two cannot disagree about what a button does (`TOOLING.md §5`).
    let screens = Screens::load(project);
    let mut text = text_engine()?;
    let mut stage = Stage::new(screens.sets(), &mut text, FACE_NAME);

    let report = vela_test::run(&module, &entry, &plans, Some(&mut stage));
    render(&session, &report, out);

    if report.is_ok() {
        return Ok(());
    }
    Err(Error::diagnostics(format!(
        "{} test(s) failed",
        report.failed()
    )))
}

/// The text engine a laid screen is sized with: the face the windowed runner uses, under the same
/// name, so a screen laid out by a test measures what it measures in a window.
fn text_engine() -> Result<TextEngine, Error> {
    let font = Font::from_bytes(DEFAULT_FACE.to_vec(), 0)
        .ok_or_else(|| Error::internal("the bundled font failed to load".to_string()))?;
    let mut text = TextEngine::new();
    text.add_font(FACE_NAME, font);
    Ok(text)
}

/// Reads the project's tests, prepares each file, and resolves where each test starts.
///
/// Two passes, because resolving `run from` needs every module's aliases and the table of modules is
/// built from the session *after* every file has been handed back — a test may start in a module that
/// comes later in the file order.
fn prepare(session: &mut Session, filter: Option<&str>) -> Vec<Plan> {
    // Read the tests, and hand each file back with the functions its expressions need. A file with no
    // tests is left alone, which is every file in most projects.
    let mut origins: Vec<(FileId, Option<String>, Vec<Plan>)> = Vec::new();
    for file in session.file_ids() {
        let Some(source) = session.sources().get(file) else {
            continue;
        };
        let (name, text) = (source.name().to_string(), source.text().to_string());

        let prepared = vela_test::prepare(file, &text);
        if prepared.plans.is_empty() {
            continue;
        }
        session.set_file(&name, &prepared.text);
        let module = session.module_of(file).map(ToString::to_string);
        origins.push((file, module, prepared.plans));
    }

    // A test that starts somewhere must start at a label in the *linked* program, so its `run from` is
    // resolved here — where the session and every module's aliases are — rather than guessed at by the
    // runner, which only has bytecode. The two names a plan carries are made absolute in the two ways
    // they need: the functions are *prefixed* with the module they were compiled into, and the start
    // label is *resolved*, which is a question about every module's aliases.
    let table = Table::of(session);
    let mut plans = Vec::new();
    for (file, module, prepared) in origins {
        for mut plan in prepared {
            if let Some(module) = &module {
                plan.qualify(module);
            }
            plan.start = start(session, &table, file, &plan);
            if filter.is_none_or(|wanted| plan.name.contains(wanted)) {
                plans.push(plan);
            }
        }
    }

    plans
}

/// The label a test's `run from` names, qualified the way the linked program names labels.
///
/// The rule is the compiler's own — `target_of`, the one function that decides what a reference points
/// at — over a table of the session's modules. A path that does not resolve is left as written, and the
/// runner reports that there is no such label, which is the truthful answer and points at the test.
fn start(session: &mut Session, table: &Table, file: FileId, plan: &Plan) -> Option<Vec<String>> {
    let path = plan.start.clone()?;
    match resolve::target(session, table, file, &path, plan.start_span) {
        Some((module, label)) => Some(vec![module.to_string(), label]),
        // Left as written, and the runner reports that there is no such label — which is true, and points
        // at the test rather than at a table this command could not build.
        None => Some(path),
    }
}

/// Writes a report: one line per test, and a location per failure.
fn render(session: &Session, report: &Report, out: &mut dyn Write) {
    for outcome in &report.outcomes {
        let verdict = if outcome.passed() { "ok  " } else { "FAIL" };
        let _ = writeln!(out, "{verdict} {}", outcome.name);

        for failure in &outcome.failures {
            let _ = writeln!(
                out,
                "  {}",
                located(session, failure.span(), &failure.message())
            );
        }
        for note in &outcome.notes {
            let _ = writeln!(out, "  note: {note}");
        }
    }

    let _ = writeln!(
        out,
        "{} passed, {} failed",
        report.passed(),
        report.failed()
    );
}

/// `path:line:column: message`, so a failure can be found by a person or by an editor.
///
/// A test failure is not a compiler diagnostic and carries no code, so it is not rendered as one: the
/// codes are permanent and reserved by phase, and inventing one for a failed assertion would put a
/// runtime fact into a table of source facts.
fn located(session: &Session, span: Span, message: &str) -> String {
    let sources = session.sources();
    let Some(state) = sources.line_col(span) else {
        return message.to_string();
    };
    let name = sources
        .get(span.file())
        .map_or_else(String::new, |source| source.name().to_string());

    format!("{name}:{}:{}: {message}", state.line + 1, state.col + 1)
}

/// One screen's focus order.
struct ScreenSweep {
    name: String,
    order: Vec<A11yNode>,
}

/// Every screen's focus order, and what is wrong across them.
#[derive(Default)]
struct Sweep {
    screens: Vec<ScreenSweep>,
    nodes: usize,
    unlabelled: Vec<String>,
}

/// Walks every screen's accessibility tree and fails on one with nothing to announce.
fn sweep(project: &Project, out: &mut dyn Write) -> Result<(), Error> {
    let sweep = focus_orders(&project.files);
    for screen in &sweep.screens {
        let _ = writeln!(
            out,
            "screen {}: {} focusable",
            screen.name,
            screen.order.len()
        );
        for node in &screen.order {
            let label = node.label.as_deref().unwrap_or("<unlabelled>");
            let _ = writeln!(
                out,
                "  [{}] {} {label:?}",
                node.focus.unwrap_or(0),
                node.role.as_str()
            );
        }
    }
    let _ = writeln!(
        out,
        "{} screen(s), {} focusable node(s), {} unlabelled",
        sweep.screens.len(),
        sweep.nodes,
        sweep.unlabelled.len()
    );

    if sweep.unlabelled.is_empty() {
        return Ok(());
    }
    // The message names what is wrong the way `W4010` does, but as a gate: the exit code is the verdict,
    // and a project that cannot be navigated by someone who cannot see it is a project that failed.
    Err(Error::diagnostics(format!(
        "{} focusable node(s) with no label: {}",
        sweep.unlabelled.len(),
        sweep.unlabelled.join(", ")
    )))
}

/// Every screen's focus order, across the project's files.
///
/// Per file, like the checker: a screen is compiled against the styles where it is declared, and the
/// accessibility tree needs nothing more than the body and the widget registry.
fn focus_orders(files: &[PathBuf]) -> Sweep {
    let registry = WidgetRegistry::builtin();
    let mut sweep = Sweep::default();

    for path in files {
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let parsed = parse(FileId::from_raw(0), &text);
        // The file's screens, so the sweep reads what a `use` draws rather than a screen's
        // declaration in isolation.
        let screens: Vec<&ScreenDecl> = parsed
            .program
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Screen(screen) => Some(screen),
                _ => None,
            })
            .collect();

        for screen in &screens {
            let mut order = Vec::new();
            focusable(
                &vela_ui::a11y::tree(&screen.body, &registry, &screens),
                &mut order,
            );

            sweep.nodes += order.len();
            for node in &order {
                if node.label.is_none() {
                    sweep
                        .unlabelled
                        .push(format!("{} ({})", screen.name, node.role.as_str()));
                }
            }
            sweep.screens.push(ScreenSweep {
                name: screen.name.clone(),
                order,
            });
        }
    }
    sweep
}

/// Collects the focusable nodes of an accessibility tree, in focus order.
///
/// Depth-first, in tree order — which `SCREENS.md §10` says *is* the focus order, so this is a
/// flattening rather than a sort, and the index each node carries is the position it lands in.
fn focusable(nodes: &[A11yNode], out: &mut Vec<A11yNode>) {
    for node in nodes {
        if node.focus.is_some() {
            out.push(node.clone());
        }
        focusable(&node.children, out);
    }
}

/// The first positional argument, skipping flags and the values they take.
fn positional(args: &[String]) -> Option<&str> {
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg.starts_with('-') {
            index += 1;
            continue;
        }
        return Some(arg);
    }
    None
}
