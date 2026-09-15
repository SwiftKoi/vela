//! `check-layers` — enforces the two rules in `docs/ARCHITECTURE.md §1`.
//!
//! Rule 1: an internal dependency must have a strictly lower rank.
//! Rule 2: no crate of rank <= 6 may depend on an adapter.

use crate::ctx::Ctx;
use crate::report::Report;
use crate::workspace::Workspace;

const NAME: &str = "check-layers";

/// Ranks at or below this may not depend on adapter crates. This is the constant that
/// keeps the front end, compiler, and VM free of graphics and platform code.
///
/// It is the highest rank of the core (front end, compiler, VM) rather than a
/// structural property, so it must be raised whenever the core's top rank moves in
/// `ARCHITECTURE.md §1`. Rank 7 is `vela-compile` and `vela-vm`.
const ADAPTER_CONSUMER_MAX_RANK: u32 = 7;

/// Runs the check.
pub fn run(ctx: &Ctx) -> Report {
    let workspace = match Workspace::load(&ctx.root) {
        Ok(ws) => ws,
        Err(err) => {
            let mut report = Report::pass(NAME, "workspace could not be loaded");
            report.violation(err);
            return report;
        }
    };

    let ranked: Vec<_> = workspace.ranked().collect();
    let edges: usize = ranked.iter().map(|c| c.internal_deps.len()).sum();
    let mut report = Report::pass(
        NAME,
        format!(
            "{} ranked crates, {edges} internal edge{}",
            ranked.len(),
            if edges == 1 { "" } else { "s" }
        ),
    );

    for krate in &ranked {
        let Some(rank) = krate.rank else {
            report.violation(format!(
                "{}: `{}` declares no rank\n  fix: add `[package.metadata.vela]` with `rank = <n>` (docs/ARCHITECTURE.md §1)",
                ctx.rel(&krate.manifest),
                krate.name
            ));
            continue;
        };

        for dep_name in &krate.internal_deps {
            let Some(dep) = workspace.find(dep_name) else {
                report.violation(format!(
                    "{}: `{}` depends on `{dep_name}`, which is not a workspace member\n  fix: correct the name, or remove the dependency",
                    ctx.rel(&krate.manifest),
                    krate.name
                ));
                continue;
            };
            check_edge(ctx, krate, rank, dep, &mut report);
        }
    }

    report
}

/// Verifies preconditions for reporting a violation against one edge.
fn check_edge(
    ctx: &Ctx,
    krate: &crate::workspace::Crate,
    rank: u32,
    dep: &crate::workspace::Crate,
    report: &mut Report,
) {
    let where_ = location(ctx, krate, &dep.name);

    // Rule 1 — rank must strictly decrease.
    match dep.rank {
        Some(dep_rank) if dep_rank >= rank => {
            report.violation(format!(
                "{where_}: `{}` (rank {rank}) depends on `{}` (rank {dep_rank})\n  rule: rank — a dependency must have a strictly lower rank\n  fix: raise `{}`'s rank above {rank}, or remove the edge",
                krate.name,
                dep.name,
                dep.name
            ));
        }
        Some(_) => {}
        None => {
            report.violation(format!(
                "{where_}: `{}` depends on `{}`, which declares no rank\n  fix: add `[package.metadata.vela]` to {}",
                krate.name,
                dep.name,
                ctx.rel(&dep.manifest)
            ));
        }
    }

    // Rule 2 — adapters are only consumable above the core.
    if dep.adapter && rank <= ADAPTER_CONSUMER_MAX_RANK {
        report.violation(format!(
            "{where_}: `{}` (rank {rank}) depends on the adapter `{}`\n  rule: adapter — no crate of rank <= {ADAPTER_CONSUMER_MAX_RANK} may depend on an adapter\n  fix: move the platform/GPU access behind a trait defined in a lower crate (CONVENTIONS.md §4.8)",
            krate.name, dep.name
        ));
    }
}

/// `path:line` of the dependency entry, for an actionable message.
fn location(ctx: &Ctx, krate: &crate::workspace::Crate, dep_name: &str) -> String {
    let path = ctx.rel(&krate.manifest);
    match krate.line_of(dep_name) {
        Some(line) => format!("{path}:{line}"),
        None => path,
    }
}
