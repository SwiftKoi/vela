//! `check-facade` — `lib.rs` and `mod.rs` declare modules and re-export; they never
//! contain logic (`docs/engineering/REPO_LAYOUT.md §2`).

use crate::ctx::Ctx;
use crate::report::Report;
use crate::scan;

const NAME: &str = "check-facade";

/// Item keywords that may not appear in a facade file.
const FORBIDDEN: &[&str] = &[
    "fn",
    "impl",
    "struct",
    "enum",
    "trait",
    "const",
    "static",
    "type",
    "macro_rules",
];

/// Runs the check.
pub fn run(ctx: &Ctx) -> Report {
    let mut report = Report::pass(NAME, "");
    let mut facades = 0usize;

    for path in scan::walk(&ctx.root, &["rs"]) {
        if !is_facade(&path) {
            continue;
        }
        facades += 1;
        let Some(text) = scan::read(&path) else {
            continue;
        };
        let rel = ctx.rel(&path);
        let blanked = scan::blank_comments(&text);

        for (i, line) in blanked.lines().enumerate() {
            let rest = strip_visibility(line.trim_start());
            if rest.is_empty() || rest.starts_with('#') {
                continue;
            }
            let token = first_token(rest);
            if FORBIDDEN.contains(&token) {
                report.violation(format!(
                    "{rel}:{}: `{token}` definition in a facade file\n  rule: REPO_LAYOUT.md §2 — lib.rs/mod.rs declare modules and re-export only\n  fix: move this into a submodule and `pub use` it here",
                    i + 1
                ));
            }
        }
    }

    report.summary = format!("{facades} facade files ({})", FACADE_NAMES.join(", "));
    report
}

const FACADE_NAMES: &[&str] = &["lib.rs", "mod.rs"];

fn is_facade(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| FACADE_NAMES.contains(&n))
}

/// Removes a leading visibility modifier so `pub fn` is recognised as `fn`.
fn strip_visibility(line: &str) -> &str {
    for prefix in ["pub(crate)", "pub(super)", "pub(self)", "pub"] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return rest.trim_start();
        }
    }
    line
}

/// The first identifier-ish token on a line.
fn first_token(line: &str) -> &str {
    line.split(|c: char| {
        c.is_whitespace() || matches!(c, '{' | '}' | ';' | '(' | ')' | '<' | ':' | '=')
    })
    .next()
    .unwrap_or("")
}
