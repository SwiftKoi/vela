//! `vela patch` — apply a delta patch to an existing build.
//!
//! `BUILD_AND_ASSETS.md §6.2`. The player already has the previous release; this puts the part
//! that changed on top of it, after checking that it is the right previous release and that
//! every byte arrived intact. Both checks come before anything is written — a half-applied
//! bundle would be neither version, and "fetch the full build instead" is only an option while
//! the old one is still there.

use std::io::Write;
use std::path::PathBuf;

use crate::command::{Command, Error};

/// The `vela patch` command.
pub struct Patch {
    base: PathBuf,
}

impl Patch {
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

    /// `vela patch apply <patch-dir> <bundle-dir>`.
    fn apply(&self, args: &[String], out: &mut dyn Write) -> Result<(), Error> {
        let paths: Vec<&str> = args
            .iter()
            .filter(|arg| !arg.starts_with('-'))
            .map(String::as_str)
            .collect();
        let (Some(patch), Some(bundle)) = (paths.first(), paths.get(1)) else {
            return Err(Error::usage(
                "`vela patch apply` needs a patch and a bundle: \
                 vela patch apply <patch-dir> <bundle-dir>",
            ));
        };

        let root = self.base.join(patch);
        let target = self.base.join(bundle);

        let patch = vela_assets::Patch::read(&root)
            .map_err(|error| Error::diagnostics(error.to_string()))?;
        patch
            .apply(&root, &target)
            .map_err(|error| Error::diagnostics(error.to_string()))?;

        let _ = writeln!(
            out,
            "applied {} file(s), removed {} -> {}/",
            patch.entries.len(),
            patch.removed.len(),
            target.display()
        );
        Ok(())
    }
}

impl Command for Patch {
    fn name(&self) -> &'static str {
        "patch"
    }

    fn about(&self) -> &'static str {
        "apply a delta patch to an existing build"
    }

    fn run(&self, args: &[String], out: &mut dyn Write) -> Result<(), Error> {
        match args.first().map(String::as_str) {
            Some("apply") => self.apply(&args[1..], out),
            Some(other) => Err(Error::usage(format!(
                "`{other}` is not a patch subcommand; the one there is, is `apply`"
            ))),
            None => Err(Error::usage(
                "`vela patch` needs a subcommand: vela patch apply <patch-dir> <bundle-dir>",
            )),
        }
    }
}
