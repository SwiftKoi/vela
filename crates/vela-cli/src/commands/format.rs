//! `vela fmt` — the canonical form of a project's files (`TOOLING.md §3`).
//!
//! The rules live in `vela-syntax::format`, which is deliberate: the formatter is a function of the
//! syntax tree, so it belongs next to the tree rather than next to the command that runs it. What
//! is here is the part that needs a filesystem — which files, whether to write them, and what to
//! say about it.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use vela_span::SourceMap;
use vela_syntax::format;

use crate::command::{Command, Error};
use crate::commands::check::collect;

/// The `vela fmt` command.
pub struct Format {
    base: PathBuf,
}

impl Format {
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

impl Command for Format {
    fn name(&self) -> &'static str {
        "fmt"
    }

    fn about(&self) -> &'static str {
        "rewrite a project's files in their canonical form"
    }

    fn run(&self, args: &[String], out: &mut dyn Write) -> Result<(), Error> {
        let target =
            positional(args).map_or_else(|| self.base.clone(), |path| self.base.join(path));
        let project = collect(&target)?;
        let check = args.iter().any(|arg| arg == "--check");
        let diff = args.iter().any(|arg| arg == "--diff");
        if check && diff {
            return Err(Error::usage(
                "`--check` says which files would change and `--diff` says how; pick one"
                    .to_string(),
            ));
        }

        let mut changed: Vec<String> = Vec::new();
        for path in &project.files {
            let shown = shown(path, &project.source_root);
            let text = fs::read_to_string(path)
                .map_err(|error| Error::usage(format!("{shown}: {error}")))?;

            // The file's own id, so a diagnostic about it can be rendered against these sources.
            let mut sources = SourceMap::new();
            let file = sources.add(shown.clone(), text.clone());

            let canonical = match format(file, &text) {
                Ok(canonical) => canonical,
                Err(not) => {
                    for diagnostic in &not.diagnostics {
                        let _ = out.write_all(vela_diag::render(diagnostic, &sources).as_bytes());
                    }
                    return Err(Error::diagnostics(format!("{shown}: {not}")));
                }
            };

            if canonical == text {
                continue;
            }
            if diff {
                // A preview, so nothing is written: the point of asking for the diff is to read it
                // before deciding.
                let _ = out.write_all(crate::diff::unified(&shown, &text, &canonical).as_bytes());
            } else if !check {
                fs::write(path, canonical)
                    .map_err(|error| Error::usage(format!("{shown}: {error}")))?;
            }
            changed.push(shown);
        }

        if changed.is_empty() {
            let _ = writeln!(out, "{} file(s) already canonical", project.files.len());
            return Ok(());
        }

        // `--check` names the files, `--diff` has already shown them, and neither writes: what they
        // share is the exit code, which is what CI reads.
        let verb = if check {
            Some("would reformat")
        } else if diff {
            None
        } else {
            Some("reformatted")
        };
        if let Some(verb) = verb {
            for path in &changed {
                let _ = writeln!(out, "{verb} {path}");
            }
        }

        if check || diff {
            // A style gate, not a correctness one (`TOOLING.md §3`): the exit code is the verdict,
            // and the message says what to run.
            return Err(Error::diagnostics(format!(
                "{} file(s) are not in canonical form; run `vela fmt`",
                changed.len()
            )));
        }
        Ok(())
    }
}

/// A path as a person reads it: relative to the sources it belongs to.
fn shown(path: &Path, source_root: &Path) -> String {
    path.strip_prefix(source_root)
        .unwrap_or(path)
        .display()
        .to_string()
}

/// The first positional argument, skipping flags.
fn positional(args: &[String]) -> Option<&str> {
    args.iter()
        .find(|arg| !arg.starts_with('-'))
        .map(String::as_str)
}
