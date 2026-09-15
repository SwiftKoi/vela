//! `vela new` — scaffold a project.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::command::{Command, Error};

/// The `vela new` command.
///
/// The base directory is a field rather than a call to `current_dir` inside `run`, so
/// the command is testable without touching the developer's working directory.
pub struct NewProject {
    base: PathBuf,
}

impl NewProject {
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

impl Command for NewProject {
    fn name(&self) -> &'static str {
        "new"
    }

    fn about(&self) -> &'static str {
        "create a new project"
    }

    fn run(&self, args: &[String], out: &mut dyn Write) -> Result<(), Error> {
        let Some(name) = args.iter().find(|a| !a.starts_with('-')) else {
            return Err(Error::usage("`vela new` needs a name: vela new <name>"));
        };
        validate(name)?;

        let dir = self.base.join(name);
        if dir.exists() {
            return Err(Error::usage(format!(
                "`{}` already exists; refusing to overwrite it",
                dir.display()
            )));
        }

        let source_dir = dir.join("src");
        fs::create_dir_all(&source_dir)
            .map_err(|e| Error::internal(format!("cannot create {}: {e}", source_dir.display())))?;

        write(&dir.join("vela.toml"), &manifest(name))?;
        write(&source_dir.join("main.vela"), MAIN_VELA)?;
        write(&dir.join(".gitignore"), GITIGNORE)?;

        let _ = writeln!(out, "created `{name}`");
        let _ = writeln!(out, "  {name}/vela.toml");
        let _ = writeln!(out, "  {name}/src/main.vela");
        Ok(())
    }
}

/// Rejects names that would escape the base directory or confuse the manifest.
fn validate(name: &str) -> Result<(), Error> {
    let valid = !name.is_empty()
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if valid {
        Ok(())
    } else {
        Err(Error::usage(format!(
            "`{name}` is not a valid project name; use letters, digits, `-`, and `_`"
        )))
    }
}

fn write(path: &Path, contents: &str) -> Result<(), Error> {
    fs::write(path, contents)
        .map_err(|e| Error::internal(format!("cannot write {}: {e}", path.display())))
}

/// `vela.toml`, schema 1.
fn manifest(name: &str) -> String {
    format!(
        "schema = 1\n\
         \n\
         [project]\n\
         name = \"{name}\"\n\
         version = \"0.1.0\"\n\
         entry = \"main.start\"\n\
         \n\
         [build]\n\
         targets = [\"web\", \"win\", \"mac\", \"linux\"]\n"
    )
}

/// The scaffolded story. Deliberately one line of dialogue: if a new project did not
/// start this small, `VISION.md §1` would already be violated.
const MAIN_VELA: &str = "\
# The entry label named by `entry` in vela.toml.

label start:
    \"Hello, world.\"
    return
";

const GITIGNORE: &str = "\
/dist
/saves
*.velac
";
