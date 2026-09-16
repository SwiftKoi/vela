//! `vela lsp` — the language server, on stdin and stdout.
//!
//! The protocol owns both streams, so nothing else may write to stdout while this runs: a stray
//! `println!` is a corrupted message and an editor that disconnects. Everything this command has to
//! say goes to stderr, or comes back as an `Error` the driver prints there.
//!
//! The workspace is loaded with the same function `vela check` uses. That is the whole point of the
//! command being here rather than in `vela-lsp`: a project is a `vela.toml`, an asset manifest, and a
//! set of sources, and the command line is what knows how to read those. Handing the loader over is
//! what keeps the editor's answer and CI's answer the same answer.

use std::io::Write;
use std::path::{Path, PathBuf};

use vela_compile::Session;
use vela_lsp::Server;

use crate::command::{Command, Error};
use crate::commands::check::{collect, load};

/// The `vela lsp` command.
pub struct Lsp {
    base: PathBuf,
}

impl Lsp {
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

impl Command for Lsp {
    fn name(&self) -> &'static str {
        "lsp"
    }

    fn about(&self) -> &'static str {
        "serve the editor protocol on stdin and stdout"
    }

    fn run(&self, args: &[String], out: &mut dyn Write) -> Result<(), Error> {
        let root = positional(args).map_or_else(|| self.base.clone(), |path| self.base.join(path));
        let start = root.display().to_string();

        // A workspace that is not a project is an empty session rather than a failed server: an editor
        // opened on a single file is still useful, and refusing to start would say otherwise.
        let loader = move |wanted: &str| match workspace(wanted) {
            Ok(session) => session,
            Err(error) => {
                eprintln!("vela lsp: {error}");
                Session::new()
            }
        };

        let mut server = Server::new(start, loader);
        let mut input = std::io::stdin().lock();
        server
            .serve(&mut input, out)
            .map_err(|error| Error::internal(format!("the language server stopped: {error}")))
    }
}

/// Loads a workspace: its sources, its entry point, and its asset manifest.
fn workspace(root: &str) -> Result<Session, Error> {
    let project = collect(Path::new(root))?;
    load(&project)
}

/// The first positional argument, skipping flags.
fn positional(args: &[String]) -> Option<&str> {
    args.iter()
        .find(|arg| !arg.starts_with('-'))
        .map(String::as_str)
}
