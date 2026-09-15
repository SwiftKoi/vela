//! `vela test` — the headless story runner, and the accessibility sweep.
//!
//! `TOOLING.md §1` lists `vela test` with `--update`, `--seed`, `--filter`, and `--a11y`. The
//! story-test modes are M10's; what exists here is **`--a11y`**, which is the M7 exit criterion
//! *"every screen passes the `--a11y` focus-order sweep"*. It walks each screen's accessibility
//! tree, reports the focus order, and fails on a focusable node with nothing to announce —
//! the same condition `W4010` lints, checked here as a gate rather than a warning.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use vela_span::FileId;
use vela_syntax::{Item, parse};
use vela_ui::WidgetRegistry;
use vela_ui::a11y::A11yNode;

use crate::command::{Command, Error};
use crate::commands::check::collect;

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
        if !args.iter().any(|arg| arg == "--a11y") {
            return Err(Error::usage(
                "story tests land in M10; for now `vela test --a11y` sweeps every screen",
            ));
        }

        let target =
            positional(args).map_or_else(|| self.base.clone(), |path| self.base.join(path));
        let project = collect(&target)?;

        let sweep = sweep(&project.files);
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
        // The message names what is wrong the way `W4010` does, but as a gate: the exit code is
        // the verdict, and a project that cannot be navigated by someone who cannot see it is a
        // project that failed its tests.
        Err(Error::diagnostics(format!(
            "{} focusable node(s) with no label: {}",
            sweep.unlabelled.len(),
            sweep.unlabelled.join(", ")
        )))
    }
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

/// Walks every screen in every file.
///
/// Per file, like the checker: a screen is compiled against the styles where it is declared,
/// and the accessibility tree needs nothing more than the body and the widget registry.
fn sweep(files: &[PathBuf]) -> Sweep {
    let registry = WidgetRegistry::builtin();
    let mut sweep = Sweep::default();

    for path in files {
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let parsed = parse(FileId::from_raw(0), &text);
        for item in &parsed.program.items {
            let Item::Screen(screen) = item else {
                continue;
            };
            let mut order = Vec::new();
            focusable(&vela_ui::a11y::tree(&screen.body, &registry), &mut order);

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
