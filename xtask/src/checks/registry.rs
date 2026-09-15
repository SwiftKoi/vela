//! The list of checks, and how to look one up.
//!
//! This lives beside `mod.rs` rather than in it because `mod.rs` is a facade by rule
//! (`REPO_LAYOUT.md §2`) — and `check-facade` enforces that on `xtask` too, which is
//! how this file came to exist.

use crate::ctx::Ctx;
use crate::report::Report;

use super::{determinism, diag_codes, exemptions, facade, file_size, layers, registries, scripts};

/// A registered architecture/policy check.
pub struct Check {
    /// Name as typed on the command line.
    pub name: &'static str,
    /// One-line description, shown by `cargo xtask --list`.
    pub about: &'static str,
    /// Entry point.
    pub run: fn(&Ctx) -> Report,
}

/// Every check, in the order CI runs them (`REPO_LAYOUT.md §6`).
pub static CHECKS: &[Check] = &[
    Check {
        name: "check-layers",
        about: "dependency ranks and adapters (ARCHITECTURE.md §1)",
        run: layers::run,
    },
    Check {
        name: "check-file-size",
        about: "file/fn/impl line budgets (REPO_LAYOUT.md §3)",
        run: file_size::run,
    },
    Check {
        name: "check-facade",
        about: "lib.rs and mod.rs contain re-exports only (REPO_LAYOUT.md §2)",
        run: facade::run,
    },
    Check {
        name: "check-exemptions",
        about: "budget exemptions carry a reason and an issue (REPO_LAYOUT.md §3.2)",
        run: exemptions::run,
    },
    Check {
        name: "check-diag-codes",
        about: "diagnostic code registry (LANGUAGE.md §8)",
        run: diag_codes::run,
    },
    Check {
        name: "check-registries",
        about: "dispatch lives in registries, not core files (CONVENTIONS.md §1)",
        run: registries::run,
    },
    Check {
        name: "check-determinism",
        about: "banned non-deterministic types and methods (REPO_LAYOUT.md §4.2)",
        run: determinism::run,
    },
    Check {
        name: "check-scripts",
        about: "the shell and JavaScript in tools/ parses (REPO_LAYOUT.md §6)",
        run: scripts::run,
    },
];

/// Looks up a check by name.
#[must_use]
pub fn find(name: &str) -> Option<&'static Check> {
    CHECKS.iter().find(|c| c.name == name)
}
