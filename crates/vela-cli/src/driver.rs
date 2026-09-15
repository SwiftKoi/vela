//! Argument parsing and dispatch.
//!
//! Exit codes are a contract with CI (`TOOLING.md §1`) and never change: 0 success,
//! 1 diagnostics present, 2 usage error, 3 internal error.

use std::io::Write;
use std::process::ExitCode;

use crate::registry::{self, Registry};

/// The engine version, taken from the package manifest.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Runs `vela`, returning the process exit code.
pub fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> ExitCode {
    ExitCode::from(run_code(args, out, err))
}

/// Runs `vela`, returning the exit code as a number.
///
/// `ExitCode` cannot be compared, so this is the entry point tests and embedders use.
pub fn run_code(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let registry = registry::builtin();

    let Some(first) = args.first() else {
        let _ = write!(out, "{}", help_text(&registry));
        return 0;
    };

    match first.as_str() {
        "--version" | "-V" => {
            let _ = writeln!(out, "vela {VERSION}");
            0
        }
        "--help" | "-h" => {
            let _ = write!(out, "{}", help_text(&registry));
            0
        }
        option if option.starts_with('-') => {
            let _ = writeln!(err, "vela: unknown option `{option}`\n");
            let _ = write!(out, "{}", help_text(&registry));
            2
        }
        name => dispatch(&registry, name, &args[1..], out, err),
    }
}

fn dispatch(
    registry: &Registry,
    name: &str,
    rest: &[String],
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let Some(command) = registry.find(name) else {
        let _ = writeln!(err, "vela: unknown command `{name}`\n");
        let _ = write!(out, "{}", help_text(registry));
        return 2;
    };

    match command.run(rest, out) {
        Ok(()) => 0,
        Err(error) => {
            // Diagnostics went to `out` as the command produced them; the explanation of
            // why the exit code is non-zero is not part of that document.
            if !error.message.is_empty() {
                let _ = writeln!(err, "vela {name}: {error}");
            }
            error.code
        }
    }
}

/// Usage text listing every registered command.
///
/// Generated from the registry rather than written by hand, so a new command cannot be
/// missing from `--help`.
#[must_use]
pub fn help_text(registry: &Registry) -> String {
    let mut text = String::from(
        "vela - a visual novel engine\n\nusage: vela <command> [options]\n\ncommands:\n",
    );
    for command in registry.iter() {
        text.push_str(&format!("  {:<10} {}\n", command.name(), command.about()));
    }
    text.push_str(
        "\noptions:\n  -h, --help     show this help\n  -V, --version  show the version\n",
    );
    text
}
