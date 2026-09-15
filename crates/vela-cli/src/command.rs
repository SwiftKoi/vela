//! The command interface every `vela` subcommand implements.

use std::fmt;
use std::io::Write;

/// A failure a command reports to the user.
///
/// The exit code is carried here rather than chosen at the call site so the mapping
/// from "what went wrong" to "what the shell sees" is decided once, in `command.rs`.
#[derive(Debug)]
pub struct Error {
    /// Message shown to the user.
    pub message: String,
    /// Process exit code: 1 diagnostics, 2 usage, 3 internal.
    pub code: u8,
}

impl Error {
    /// The user asked for something impossible or malformed (exit 2).
    #[must_use]
    pub fn usage(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: 2,
        }
    }

    /// The project has errors that must be fixed (exit 1).
    #[must_use]
    pub fn diagnostics(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: 1,
        }
    }

    /// A bug in `vela` itself (exit 3).
    #[must_use]
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: 3,
        }
    }

    /// A failure the command has already explained in its own output.
    ///
    /// Used when the output is a document a tool will parse: the exit code carries the
    /// verdict, and a summary appended to the document would break it.
    #[must_use]
    pub fn silent() -> Self {
        Self {
            message: String::new(),
            code: 1,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

/// A `vela` subcommand.
///
/// Commands are thin adapters over the engine crates; behavior belongs in those crates
/// so it can be tested without a terminal.
pub trait Command {
    /// The name typed on the command line.
    fn name(&self) -> &'static str;

    /// One-line description, shown by `vela --help`.
    fn about(&self) -> &'static str;

    /// Runs the command with the arguments that followed its name.
    fn run(&self, args: &[String], out: &mut dyn Write) -> Result<(), Error>;
}
