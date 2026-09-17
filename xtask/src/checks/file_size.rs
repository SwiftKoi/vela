//! `check-file-size` — enforces the budget table in `docs/engineering/REPO_LAYOUT.md §3`.
//!
//! This is the mechanism behind VISION principle 4. It is mechanical on purpose: a rule
//! that depends on reviewers remembering it is a rule that decays.
//!
//! **Code only.** Source files, `lib.rs`/`mod.rs`, `fn` and `impl` bodies, and a crate's
//! `[dependencies]` are measured, because those budgets stand for something structural: how much of
//! one file a reader has to hold to change it, and how many things a crate is doing. A document is
//! not like that. Its length is an editorial judgement — a spec is long because the contract is — and
//! a line count cannot tell a well-argued 700 from a padded one. So prose is not counted here; the
//! one-sitting rule still guides a reader, and a reviewer is the one who applies it.

use crate::ctx::Ctx;
use crate::report::Report;
use crate::scan::{self, BlockKind};
use crate::workspace::Workspace;

const NAME: &str = "check-file-size";

/// Marker that exempts a file from its budget; validity is enforced by
/// `check-exemptions`.
const EXEMPT_MARKER: &str = "vela-exempt:";

/// A warn/hard line budget.
struct Budget {
    warn: usize,
    hard: usize,
}

const RS: Budget = Budget {
    warn: 400,
    hard: 500,
};
const FACADE: Budget = Budget {
    warn: 80,
    hard: 120,
};
const FN: Budget = Budget { warn: 60, hard: 80 };
const IMPL: Budget = Budget {
    warn: 200,
    hard: 300,
};

/// Entries in a crate's `[dependencies]`.
const DEPS: Budget = Budget { warn: 25, hard: 35 };

const SPLIT_RECIPE: &str = concat!(
    "  fix: apply the split recipe in REPO_LAYOUT.md §3.1 — ",
    "(1) split by phase, (2) split by variant, (3) split by responsibility, ",
    "(4) extract a registry, (5) extract a sub-crate"
);

/// Runs the check.
pub fn run(ctx: &Ctx) -> Report {
    let rust = scan::walk(&ctx.root, &["rs"]);

    let mut report = Report::pass(NAME, "");
    let mut warnings = 0usize;
    let mut exempt = 0usize;

    for path in &rust {
        let Some(text) = scan::read(path) else {
            continue;
        };
        let rel = ctx.rel(path);

        // Scan once: the blanked copy is needed for block measurement, and the
        // comments are needed to find a genuine exemption.
        let scanned = scan::scan_source(&text);
        if scanned
            .comments
            .iter()
            .any(|c| c.body.contains(EXEMPT_MARKER))
        {
            exempt += 1;
            continue;
        }

        let budget = if is_facade(path) { &FACADE } else { &RS };
        let lines = scan::line_count(&text);
        if lines > budget.hard {
            report.violation(format!(
                "{rel}: {lines} lines exceeds the hard limit of {}\n{SPLIT_RECIPE}",
                budget.hard
            ));
        } else if lines > budget.warn {
            warnings += 1;
        }

        check_blocks(&rel, &scanned.blanked, &mut report, &mut warnings);
    }

    if let Ok(workspace) = Workspace::load(&ctx.root) {
        for krate in &workspace.crates {
            if krate.normal_dep_count > DEPS.hard {
                report.violation(format!(
                    "{}: {} entries in [dependencies], hard limit is {}\n  fix: a crate with this many dependencies is doing too much; split it along the seam the extras reveal",
                    ctx.rel(&krate.manifest),
                    krate.normal_dep_count,
                    DEPS.hard
                ));
            } else if krate.normal_dep_count > DEPS.warn {
                warnings += 1;
            }
        }
    }

    report.summary = format!(
        "{} Rust file(s), {warnings} over the warn threshold, {exempt} exempt",
        rust.len()
    );
    report
}

/// Checks each `fn`/`impl` block against its budget.
fn check_blocks(rel: &str, blanked: &str, report: &mut Report, warnings: &mut usize) {
    // Braces inside string literals must not count: a test asserting on `"{a} {b}"`
    // would otherwise look like a function spanning the whole file.
    let measure = scan::blank_literals(blanked);
    for block in scan::find_blocks(&measure) {
        let (budget, label) = match block.kind {
            BlockKind::Fn => (&FN, "function body"),
            BlockKind::Impl => (&IMPL, "impl block"),
        };
        if block.lines > budget.hard {
            report.violation(format!(
                "{rel}:{}: {label} spans {} lines, hard limit is {}\n  fix: extract helpers so each piece fits on one screen",
                block.start_line, block.lines, budget.hard
            ));
        } else if block.lines > budget.warn {
            *warnings += 1;
        }
    }
}

/// Whether a file is a facade (`lib.rs` / `mod.rs`), which has a tighter budget.
fn is_facade(path: &std::path::Path) -> bool {
    matches!(
        path.file_name().and_then(|n| n.to_str()),
        Some("lib.rs" | "mod.rs")
    )
}
