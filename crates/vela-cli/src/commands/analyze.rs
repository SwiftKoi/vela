//! `vela analyze` — what the story graph says about the story.
//!
//! `TOOLING.md §7` asks for reports that turn the compile-time graph into human decisions, and for
//! output that is *deterministic and diffable* so a metric can be tracked over time in CI — "our
//! unreachable-label count went up by three this week". That second requirement decides the shape of
//! everything here: no hashing anywhere, every list sorted, and one label per line of the report.
//!
//! # What it reports, and what it leaves to the checker
//!
//! The graph, and what a run from the entry cannot reach. Two of the reports the spec names are already
//! `vela check`'s: an unreachable label is `W4002`, and a label that can end without transferring control
//! is `W4003`. Re-deriving them here would give the project two answers to one question, so the `reached`
//! flag below is a *graph* fact — the same answer, computed the same way, from the same resolver — and the
//! diagnostics stay the checker's to report with spans and advice.
//!
//! # What it does not do yet
//!
//! Unused assets (`W7001`, which does not exist), per-scene load sizes, localization coverage, and
//! variable reachability. Each needs something the project does not have yet — an asset manifest that
//! survives a check, a text pipeline, a liveness pass over `default`s — and naming them is the point:
//! a report that quietly omits a section reads as "nothing to report".

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::PathBuf;

use serde_json::{Value, json};
use vela_compile::Session;
use vela_diag::Severity;
use vela_span::Span;

use crate::command::{Command, Error};
use crate::commands::check::{Project, collect, load};
use crate::commands::resolve::{self, Table};
use crate::commands::run::flag_value;

/// The `vela analyze` command.
pub struct Analyze {
    base: PathBuf,
}

impl Analyze {
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

impl Command for Analyze {
    fn name(&self) -> &'static str {
        "analyze"
    }

    fn about(&self) -> &'static str {
        "report the story graph, and what a run cannot reach"
    }

    fn run(&self, args: &[String], out: &mut dyn Write) -> Result<(), Error> {
        let target =
            positional(args).map_or_else(|| self.base.clone(), |path| self.base.join(path));
        let project = collect(&target)?;
        let mut session = load(&project)?;

        // A graph is only worth reading if the project compiled: an unresolved reference would appear as
        // a missing edge, which reads as "nothing goes there" rather than "this does not compile".
        if session
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.severity() == Severity::Error)
        {
            return Err(Error::diagnostics(
                "the project has errors; run `vela check` to see them".to_string(),
            ));
        }

        let entry = entry(&project)?;
        let graph = Graph::of(&mut session, &entry);

        match flag_value(args, "--format").unwrap_or("text") {
            "json" => {
                let _ = writeln!(out, "{}", graph.json());
            }
            "dot" => {
                let _ = write!(out, "{}", graph.dot());
            }
            "text" => {
                let _ = write!(out, "{}", graph.text());
            }
            other => {
                return Err(Error::usage(format!(
                    "unknown format `{other}`; expected text, json, or dot"
                )));
            }
        }
        Ok(())
    }
}

/// The project's entry point, which is where reachability starts.
fn entry(project: &Project) -> Result<String, Error> {
    project
        .manifest
        .as_ref()
        .map(|manifest| manifest.project.entry.clone())
        .ok_or_else(|| Error::usage("no entry point: set `entry` in vela.toml".to_string()))
}

/// One label, as the analysis sees it.
struct Node {
    /// The label, qualified the way the linked program names it.
    name: String,
    /// Where it was written.
    file: String,
    /// 1-based, like everything else in this repository's reports.
    line: u32,
    /// 1-based.
    column: u32,
    /// Every label this one can transfer control to, sorted and without repeats.
    targets: Vec<String>,
    /// Whether a run from the entry can get here.
    reached: bool,
}

/// A whole project's story graph: every module's labels, as one graph.
struct Graph {
    /// The label a run starts at.
    entry: String,
    /// Every label, by name. A `BTreeMap` until the end, so the order is the name order.
    nodes: Vec<Node>,
}

impl Graph {
    /// Builds the graph, resolving every edge the way the compiler resolves it.
    fn of(session: &mut Session, entry: &str) -> Self {
        let table = Table::of(session);
        let mut nodes: BTreeMap<String, Node> = BTreeMap::new();
        let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

        for file in session.file_ids() {
            // Cloned rather than borrowed, because resolving below needs the session again.
            let collected = session.symbols(file);
            let module = collected.module.name.to_string();

            for label in &collected.module.story.nodes {
                let name = format!("{module}.{}", label.name);
                let mut targets = BTreeSet::new();
                for reference in &label.targets {
                    if let Some((other, target)) =
                        resolve::target(session, &table, file, &reference.path, reference.span)
                    {
                        targets.insert(format!("{other}.{target}"));
                    }
                }

                let (file_name, line, column) = position(session, label.span);
                edges.insert(name.clone(), targets.clone());
                nodes.insert(
                    name.clone(),
                    Node {
                        name,
                        file: file_name,
                        line,
                        column,
                        targets: targets.into_iter().collect(),
                        reached: false,
                    },
                );
            }
        }

        // Reachability from the entry, breadth-first over the resolved edges. An entry that is not a
        // label — a typo in the manifest — reaches nothing, and every label is then reported unreachable,
        // which is the truthful reading of a graph with no start.
        let mut reached: BTreeSet<String> = BTreeSet::new();
        let mut queue: Vec<String> = Vec::new();
        if nodes.contains_key(entry) {
            queue.push(entry.to_string());
            reached.insert(entry.to_string());
        }
        while let Some(name) = queue.pop() {
            for target in edges.get(&name).into_iter().flatten() {
                if reached.insert(target.clone()) {
                    queue.push(target.clone());
                }
            }
        }
        for (name, node) in &mut nodes {
            node.reached = reached.contains(name);
        }

        Self {
            entry: entry.to_string(),
            nodes: nodes.into_values().collect(),
        }
    }

    /// How many edges the graph has, counting each distinct target once.
    fn edges(&self) -> usize {
        self.nodes.iter().map(|node| node.targets.len()).sum()
    }

    /// Labels with nowhere to go: the leaves of the graph.
    fn terminal(&self) -> usize {
        self.nodes
            .iter()
            .filter(|node| node.targets.is_empty())
            .count()
    }

    /// Labels a run from the entry cannot get to.
    fn unreachable(&self) -> usize {
        self.nodes.iter().filter(|node| !node.reached).count()
    }

    /// The documented JSON shape (`TOOLING.md §7`): one object, every list sorted.
    fn json(&self) -> String {
        let labels: Vec<Value> = self
            .nodes
            .iter()
            .map(|node| {
                json!({
                    "name": node.name,
                    "file": node.file,
                    "line": node.line,
                    "column": node.column,
                    "reached": node.reached,
                    "targets": node.targets,
                })
            })
            .collect();

        let document = json!({
            "schema": 1,
            "entry": self.entry,
            "labels": labels,
            "totals": {
                "labels": self.nodes.len(),
                "edges": self.edges(),
                "reached": self.nodes.len() - self.unreachable(),
                "unreachable": self.unreachable(),
                "terminal": self.terminal(),
            },
        });

        serde_json::to_string_pretty(&document).unwrap_or_else(|_| "{}".to_string())
    }

    /// The graph as `dot`, for `dot -Tsvg`.
    fn dot(&self) -> String {
        let mut out = format!(
            "// entry: {}\ndigraph story {{\n  rankdir=LR;\n  node [shape=box];\n",
            self.entry
        );
        for node in &self.nodes {
            if !node.reached {
                // An unreachable label is drawn but dashed: the picture should show that nothing gets
                // there rather than leave it out of a diagram about what the story does.
                out.push_str(&format!("  \"{}\" [style=dashed];\n", node.name));
            }
            for target in &node.targets {
                out.push_str(&format!("  \"{}\" -> \"{}\";\n", node.name, target));
            }
        }
        out.push_str("}\n");
        out
    }

    /// The graph as one line per label, which is the form a diff reads best.
    fn text(&self) -> String {
        let mut out = String::new();
        let width = self
            .nodes
            .iter()
            .map(|node| node.name.len())
            .max()
            .unwrap_or(0);

        for node in &self.nodes {
            let mark = if node.reached { " " } else { "!" };
            out.push_str(&format!(
                "{mark} {:width$}  {}:{}\n",
                node.name,
                node.file,
                node.line,
                width = width
            ));
            for target in &node.targets {
                out.push_str(&format!("      -> {target}\n"));
            }
        }

        out.push_str(&format!(
            "{}, {}, {} reached, {} unreachable, {} terminal\n",
            count(self.nodes.len(), "label"),
            count(self.edges(), "edge"),
            self.nodes.len() - self.unreachable(),
            self.unreachable(),
            self.terminal()
        ));
        out
    }
}

/// A count with its noun, so a report never says "1 edges".
fn count(number: usize, noun: &str) -> String {
    if number == 1 {
        format!("{number} {noun}")
    } else {
        format!("{number} {noun}s")
    }
}

/// Where a span is written, as `(file, line, column)` with a 1-based line and column.
fn position(session: &Session, span: Span) -> (String, u32, u32) {
    let sources = session.sources();
    let Some(file) = sources.get(span.file()) else {
        return (String::new(), 0, 0);
    };
    let at = file.line_col(span.start());
    (file.name().to_string(), at.line + 1, at.col + 1)
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
