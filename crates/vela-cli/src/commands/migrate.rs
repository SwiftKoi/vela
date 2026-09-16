//! `vela migrate` — Ren'Py in, Vela out.
//!
//! `TOOLING.md §8`: the transpiler handles the common case and *reports* the rest precisely. So
//! this command writes a project beside `MIGRATION.md`, which is the report — the report is the
//! rest of the work, and a work item list that scrolls off a terminal is one nobody keeps.
//!
//! `--report` prints it, `--strict` makes a non-empty one an exit code, and both exist so a team
//! can watch the number go to zero in CI rather than reading a wall of text once.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::command::{Command, Error};
use crate::commands::run::{flag_value, positional};

/// The `vela migrate` command.
pub struct Migrate {
    base: PathBuf,
}

impl Migrate {
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

impl Command for Migrate {
    fn name(&self) -> &'static str {
        "migrate"
    }

    fn about(&self) -> &'static str {
        "transpile a Ren'Py project into a Vela one"
    }

    fn run(&self, args: &[String], out: &mut dyn Write) -> Result<(), Error> {
        let target =
            positional(args).map_or_else(|| self.base.clone(), |path| self.base.join(path));
        let project =
            vela_migrate::project(&target).map_err(|error| Error::usage(error.to_string()))?;

        let into = flag_value(args, "--out").map_or_else(|| default_out(&target), PathBuf::from);
        project
            .write(Path::new(&into))
            .map_err(|error| Error::internal(error.to_string()))?;

        let _ = writeln!(
            out,
            "migrated {} source file(s) to {} ({} thing(s) to port by hand)",
            project.files.len(),
            into.display(),
            project.report.len()
        );

        if args.iter().any(|arg| arg == "--report") {
            let _ = writeln!(out, "\n{}", project.report.render());
        }

        // `--strict` is the number a team can track: a migration is done when this exits zero.
        if args.iter().any(|arg| arg == "--strict") && !project.report.is_empty() {
            return Err(Error::diagnostics(format!(
                "{} construct(s) have no automatic translation; see {}",
                project.report.len(),
                Path::new(&into).join("MIGRATION.md").display()
            )));
        }
        Ok(())
    }
}

/// Where a migration goes when `--out` does not say: beside the project it came from.
fn default_out(target: &Path) -> PathBuf {
    let name = target.file_name().map_or_else(
        || "migrated".to_string(),
        |name| name.to_string_lossy().to_string(),
    );
    target.with_file_name(format!("{name}_migrated"))
}
