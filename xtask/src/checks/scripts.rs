//! `check-scripts` — the scripts in `tools/` parse (`REPO_LAYOUT.md §6`).
//!
//! Naming a script language is not compiling it. Rust is checked by `cargo build` on every run;
//! a shell or JavaScript file is checked by nothing until the gate that *uses* it runs, and in CI
//! that is the last step of a long pipeline. The failure that prompted this check was exactly that:
//! `tools/wasm-smoke.cjs` declared `const module`, which in CommonJS is not a shadow but a
//! `SyntaxError` — `module` is the name the wrapper already binds for the file's own exports — and
//! the only place it could be noticed was the wasm job, several minutes in.
//!
//! `bash -n` and `node --check` parse without executing, which is what was missing. A tool that is
//! not installed is reported as *unchecked* rather than as passing: a check that quietly skips is
//! how a check stops being one, and the summary is where that has to be visible.

use std::path::Path;
use std::process::Command;

use crate::ctx::Ctx;
use crate::report::Report;
use crate::scan;

const NAME: &str = "check-scripts";

/// How many lines of a parser's complaint to keep.
const KEEP_LINES: usize = 3;

/// One script language: its extension, and the tool that parses it without running it.
struct Language {
    /// File extension, without the dot.
    extension: &'static str,
    /// The tool that knows the syntax.
    tool: &'static str,
    /// Arguments that make it parse and stop.
    args: &'static [&'static str],
}

/// The languages the repository keeps scripts in.
const LANGUAGES: &[Language] = &[
    Language {
        extension: "sh",
        tool: "bash",
        args: &["-n"],
    },
    Language {
        extension: "cjs",
        tool: "node",
        args: &["--check"],
    },
    Language {
        extension: "js",
        tool: "node",
        args: &["--check"],
    },
    Language {
        extension: "mjs",
        tool: "node",
        args: &["--check"],
    },
];

/// Runs the check.
pub fn run(ctx: &Ctx) -> Report {
    let mut report = Report::pass(NAME, "");
    let mut scripts = ctx.root.join("tools");
    if !scripts.is_dir() {
        report.summary = "no tools/ directory".to_string();
        return report;
    }
    scripts = scripts.canonicalize().unwrap_or(scripts);

    let mut parsed = 0usize;
    let mut used: Vec<&'static str> = Vec::new();
    let mut missing: Vec<&'static str> = Vec::new();

    for language in LANGUAGES {
        let files = scan::walk(&scripts, &[language.extension]);
        if files.is_empty() {
            continue;
        }
        if !installed(language.tool) {
            if !missing.contains(&language.tool) {
                missing.push(language.tool);
            }
            continue;
        }
        if !used.contains(&language.tool) {
            used.push(language.tool);
        }

        for path in files {
            let rel = ctx.rel(&path);
            if let Some(complaint) = parse_failure(language, &path, &rel) {
                report.violation(format!(
                    "{rel}: {complaint}\n  rule: REPO_LAYOUT.md §6 — a script is a gate, and a gate that does not parse never runs\n  fix: run `{} {} {rel}`",
                    language.tool,
                    language.args.join(" ")
                ));
            }
            parsed += 1;
        }
    }

    report.summary = summarise(parsed, &used, &missing);
    report
}

/// Whether a tool can be run at all.
fn installed(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

/// The parser's complaint about `path`, or `None` if it parsed.
///
/// The tool prints the path it was given, and that is an absolute path on this machine — so it is
/// rewritten to the relative one, which is what keeps the check's output identical across
/// machines.
fn parse_failure(language: &Language, path: &Path, rel: &str) -> Option<String> {
    let output = Command::new(language.tool)
        .args(language.args)
        .arg(path)
        .output()
        .ok()?;
    if output.status.success() {
        return None;
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let complaint = first_lines(&stderr);
    let absolute = path.display().to_string();
    Some(complaint.replace(&absolute, rel))
}

/// The first few lines worth reading from a parser's output.
fn first_lines(stderr: &str) -> String {
    stderr
        .lines()
        .map(str::trim)
        .filter(|line| informative(line))
        .take(KEEP_LINES)
        .collect::<Vec<_>>()
        .join("; ")
}

/// Whether a parser's line says something about the *script*.
///
/// Node's real complaint arrives after warnings about how it noticed one — it retries a CommonJS
/// file as an ES module, fails, and says so — and after the offending source line, a caret, and a
/// stack trace. What a reader needs is the location and the error, so everything else is dropped:
/// the first version of this kept three lines, and two of them were the warning.
fn informative(line: &str) -> bool {
    !line.is_empty()
        && !line.chars().all(|c| c == '^' || c.is_whitespace())
        && !line.starts_with("at ")
        && !line.starts_with("(node:")
        && !line.starts_with("(Use")
        && (line.contains(':') || line.contains("rror"))
}

/// The one-line summary, including what could not be checked.
fn summarise(parsed: usize, used: &[&str], missing: &[&str]) -> String {
    let mut summary = if used.is_empty() {
        format!("{parsed} script(s), none checked")
    } else {
        format!("{parsed} script(s) parsed ({})", used.join(", "))
    };
    if !missing.is_empty() {
        summary.push_str(&format!(
            "; {} not installed, {} unchecked by it",
            missing.join(", "),
            missing.len()
        ));
    }
    summary
}
