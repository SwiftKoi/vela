//! `check-format` — every `.vela` file in the repository is already in canonical form
//! (`TOOLING.md §3`).
//!
//! The formatter's rules are pinned by tests over the corpus; what this checks is the *corpus
//! itself*, which is the other half of the same claim and the one CI can enforce. Two reasons it is
//! worth a gate rather than a habit:
//!
//! - a file that is not canonical makes `vela fmt` a change in every diff that touches it, and
//!   "run the formatter first" is exactly the instruction a gate can save a reviewer from writing;
//! - the first time this check was run by hand it found a real bug. `expr_forms.vela` prints
//!   `not x and y or z`, the formatter wanted `!x and y or z`, and those are *different programs* —
//!   `not` binds looser than a comparison and `!` tighter than any binary operator. The corpus is
//!   the only place that had ever seen `not` in a position where the difference shows.
//!
//! A file that does not parse is skipped rather than failed: the formatter refuses to touch one
//! (`E0xxx`/`E1xxx` fixtures are *meant* to be broken), and reporting them here would mean
//! whitelisting them by name.

use vela_span::FileId;
use vela_syntax::format;

use crate::ctx::Ctx;
use crate::report::Report;
use crate::scan;

const NAME: &str = "check-format";

/// Runs the check.
pub fn run(ctx: &Ctx) -> Report {
    let mut report = Report::pass(NAME, "");
    let mut canonical = 0usize;
    let mut refused = 0usize;

    for path in scan::walk(&ctx.root, &["vela"]) {
        let Some(source) = scan::read(&path) else {
            continue;
        };
        let rel = ctx.rel(&path);

        let Ok(formatted) = format(FileId::from_raw(0), &source) else {
            refused += 1;
            continue;
        };
        canonical += 1;

        if formatted != source {
            report.violation(format!(
                "{rel}: not in canonical form\n  rule: TOOLING.md §3 — one canonical form per file, so a diff shows what changed rather than how it was typed\n  fix: run `vela fmt {rel}`"
            ));
        }
    }

    report.summary =
        format!("{canonical} file(s) canonical, {refused} not parsed (and so not formatted)");
    report
}
