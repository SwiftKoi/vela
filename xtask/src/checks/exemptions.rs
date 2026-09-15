//! `check-exemptions` — a budget exemption must carry a reason and an issue reference,
//! and the current count is reported on every run so exemptions stay visible
//! (`docs/engineering/REPO_LAYOUT.md §3.2`).

use crate::ctx::Ctx;
use crate::report::Report;
use crate::scan;

const NAME: &str = "check-exemptions";

/// The marker a check-file-size exemption is written with.
const MARKER: &str = "vela-exempt:";

/// Runs the check.
pub fn run(ctx: &Ctx) -> Report {
    let mut report = Report::pass(NAME, "");
    let mut count = 0usize;

    for path in scan::walk(&ctx.root, &["rs"]) {
        let Some(text) = scan::read(&path) else {
            continue;
        };
        let rel = ctx.rel(&path);

        // Only real line comments count. A doc comment or a string literal that
        // mentions the marker is describing it, not using it.
        for comment in scan::scan_source(&text).comments {
            let Some(pos) = comment.body.find(MARKER) else {
                continue;
            };
            count += 1;
            let after = comment.body[pos + MARKER.len()..].trim();
            let (reason, issue) = split_issue(after);

            if reason.trim().is_empty() {
                report.violation(format!(
                    "{rel}:{}: exemption has no reason\n  format: `// vela-exempt: <reason> (#<issue>)`",
                    comment.line
                ));
            }
            if issue.is_none() {
                report.violation(format!(
                    "{rel}:{}: exemption has no issue reference\n  format: `// vela-exempt: <reason> (#<issue>)`",
                    comment.line
                ));
            }
        }
    }

    report.summary = format!("{count} exemption(s) in the workspace");
    report
}

/// Splits an exemption body into its reason and its trailing `(#123)` issue reference.
fn split_issue(body: &str) -> (&str, Option<&str>) {
    let Some(open) = body.rfind("(#") else {
        return (body, None);
    };
    let rest = &body[open + 2..];
    let Some(close) = rest.find(')') else {
        return (body, None);
    };
    let digits = &rest[..close];
    let valid = !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit());
    if valid {
        (&body[..open], Some(digits))
    } else {
        (body, None)
    }
}
