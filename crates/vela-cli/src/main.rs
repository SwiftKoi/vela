//! The `vela` binary: collect arguments, dispatch, exit.
//!
//! Deliberately thin. Everything testable lives in the library, because a `main` that
//! does work is a `main` that cannot be tested.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out = std::io::stdout();
    let mut err = std::io::stderr();
    vela_cli::run(&args, &mut out, &mut err)
}
