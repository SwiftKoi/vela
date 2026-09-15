//! `check-determinism` — a path-aware scan for the banned types and methods in
//! `docs/engineering/REPO_LAYOUT.md §4.2`.
//!
//! This is a source scan rather than a `clippy.toml` because clippy config is
//! workspace-global and cannot express the one legitimate exception (`vela-host` owns
//! the clock). Comments are blanked before scanning, so documenting a banned call is
//! fine — only writing one is a violation.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::ctx::Ctx;
use crate::report::Report;
use crate::scan;

const NAME: &str = "check-determinism";

/// Banned substrings and why each is banned.
const PATTERNS: &[(&str, &str)] = &[
    (
        "SystemTime::now",
        "wall-clock time must be injected, not read",
    ),
    ("Instant::now", "monotonic time must be injected, not read"),
    ("thread_rng", "RNG must live in World so replay is exact"),
    ("rand::random", "RNG must live in World so replay is exact"),
    (
        "RandomState",
        "unordered hashing makes iteration order unspecified",
    ),
    ("HashMap<", "unordered iteration; use IndexMap or BTreeMap"),
    ("HashSet<", "unordered iteration; use IndexSet or BTreeSet"),
];

const ALLOWLIST: &str = "xtask/determinism-allowlist.toml";

#[derive(Debug, Default, Deserialize)]
struct Allowlist {
    #[serde(default)]
    allow: BTreeMap<String, String>,
}

/// Runs the check.
pub fn run(ctx: &Ctx) -> Report {
    let allowed = load_allowlist(&ctx.root);
    let mut report = Report::pass(NAME, "");
    let mut scanned = 0usize;

    for path in scan::walk(&ctx.root.join("crates"), &["rs"]) {
        let rel = ctx.rel(&path);
        let Some(krate) = crate_of(&rel) else {
            continue;
        };
        if allowed.contains_key(krate) {
            continue;
        }
        let Some(text) = scan::read(&path) else {
            continue;
        };
        scanned += 1;

        let blanked = scan::blank_comments(&text);
        for (i, line) in blanked.lines().enumerate() {
            for (pattern, why) in PATTERNS {
                if line.contains(pattern) {
                    report.violation(format!(
                        "{rel}:{}: `{pattern}` — {why}\n  fix: see REPO_LAYOUT.md §4.2; if this is a genuine exception, add the crate to {ALLOWLIST} with a justification",
                        i + 1
                    ));
                }
            }
        }
    }

    report.summary = format!(
        "{scanned} files scanned, {} crate(s) allowlisted",
        allowed.len()
    );
    report
}

/// Extracts the crate name from a path relative to the workspace root.
fn crate_of(rel: &str) -> Option<&str> {
    let mut parts = rel.split('/');
    match (parts.next(), parts.next()) {
        (Some("crates"), Some(name)) if !name.is_empty() => Some(name),
        _ => None,
    }
}

fn load_allowlist(root: &Path) -> BTreeMap<String, String> {
    let path = root.join(ALLOWLIST);
    let Some(text) = scan::read(&path) else {
        return BTreeMap::new();
    };
    toml::from_str::<Allowlist>(&text)
        .map(|a| a.allow)
        .unwrap_or_default()
}
