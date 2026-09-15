//! Architecture and policy checks for the vela workspace.
//!
//! Run as `cargo xtask <check> [<check>...]`. Every check is registered in
//! `checks::CHECKS`; adding one is a table entry plus a new file, never an edit to
//! the dispatcher.

mod bless;
mod budget;
mod checks;
mod ctx;
mod report;
mod scan;
mod workspace;

use std::process::ExitCode;

use ctx::Ctx;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let ctx = Ctx::new();

    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        print_usage();
        return ExitCode::SUCCESS;
    }

    // `bless` is not a check: it rewrites files rather than judging them, so it sits
    // outside the report protocol.
    // `budget` measures rather than judges the tree, so it sits beside `bless` and outside
    // the check protocol — and deliberately outside `all`, which must stay quick.
    if args[0] == "budget" {
        return if budget::run() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }

    if args[0] == "bless" {
        return if bless::run() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }

    let names = select(&args);
    if names.is_empty() {
        return ExitCode::from(2);
    }

    let mut failed = 0usize;
    for name in names {
        let Some(check) = checks::find(&name) else {
            eprintln!("xtask: unknown check `{name}`");
            print_usage();
            return ExitCode::from(2);
        };
        let report = (check.run)(&ctx);
        report::print(&report);
        if report.failed() {
            failed += 1;
        }
    }

    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        println!("\n{failed} check(s) failed");
        ExitCode::FAILURE
    }
}

/// Resolves the requested check names, expanding `all` and `--list`.
fn select(args: &[String]) -> Vec<String> {
    match args[0].as_str() {
        "all" => checks::CHECKS.iter().map(|c| c.name.to_string()).collect(),
        "--list" | "list" => {
            print_usage();
            Vec::new()
        }
        _ => args.to_vec(),
    }
}

fn print_usage() {
    println!("cargo xtask <check> [<check>...]");
    println!("cargo xtask all");
    println!("cargo xtask bless\n");
    println!("cargo run --release -p xtask -- budget\n");
    println!("checks:");
    for check in checks::CHECKS {
        println!("  {:<18} {}", check.name, check.about);
    }
    println!("\nworkspace root: {}", Ctx::new().root.display());
}
