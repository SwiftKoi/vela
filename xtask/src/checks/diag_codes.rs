//! `check-diag-codes` — the diagnostic code registry (`docs/spec/LANGUAGE.md §8`).
//!
//! Validates shape, severity agreement, the phase ranges, sort order, uniqueness, and
//! that every registered code is exercised by at least one test.

use std::collections::BTreeSet;
use std::path::Path;

use crate::ctx::Ctx;
use crate::report::Report;
use crate::scan;

const NAME: &str = "check-diag-codes";

/// The registry file, relative to the workspace root.
const REGISTRY: &str = "crates/vela-diag/codes.txt";

/// Runs the check.
pub fn run(ctx: &Ctx) -> Report {
    let mut report = Report::pass(NAME, "");

    let Some(text) = scan::read(&ctx.root.join(REGISTRY)) else {
        report.violation(format!(
            "{REGISTRY}: not found\n  fix: create it with one `<code>|<severity>|<title>` per line"
        ));
        report.summary = "registry missing".to_string();
        return report;
    };

    let mut codes: Vec<(String, usize)> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();

    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let num = i + 1;
        let mut parts = line.split('|');
        let (Some(code), Some(severity), Some(title)) = (parts.next(), parts.next(), parts.next())
        else {
            report.violation(format!(
                "{REGISTRY}:{num}: expected `<code>|<severity>|<title>`"
            ));
            continue;
        };
        let (code, severity, title) = (code.trim(), severity.trim(), title.trim());

        validate(&mut report, num, code, severity, title);
        if !seen.insert(code.to_string()) {
            report.violation(format!("{REGISTRY}:{num}: `{code}` is declared twice"));
        }
        codes.push((code.to_string(), num));
    }

    for pair in codes.windows(2) {
        if pair[0].0 >= pair[1].0 {
            report.violation(format!(
                "{REGISTRY}:{}: `{}` should sort before `{}`\n  fix: keep the registry sorted by code",
                pair[1].1, pair[1].0, pair[0].0
            ));
        }
    }

    for (code, num) in &codes {
        if !referenced_in_test(&ctx.root, code) {
            report.violation(format!(
                "{REGISTRY}:{num}: `{code}` has no test referencing it\n  rule: every code is exercised by a test (CONVENTIONS.md §3)"
            ));
        }
    }

    report.summary = format!("{} codes registered", codes.len());
    report
}

/// Validates a single entry's shape and range.
fn validate(report: &mut Report, num: usize, code: &str, severity: &str, title: &str) {
    if !valid_format(code) {
        report.violation(format!(
            "{REGISTRY}:{num}: `{code}` is not `[EWL]` followed by four digits"
        ));
        return;
    }
    if !matches!(severity, "error" | "warning" | "lint") {
        report.violation(format!(
            "{REGISTRY}:{num}: severity `{severity}` is not error, warning, or lint"
        ));
        return;
    }
    if title.is_empty() {
        report.violation(format!("{REGISTRY}:{num}: `{code}` has an empty title"));
    }

    let expected = match code.as_bytes()[0] {
        b'E' => "error",
        b'W' => "warning",
        _ => "lint",
    };
    if severity != expected {
        report.violation(format!(
            "{REGISTRY}:{num}: `{code}` is a `{expected}` code but is declared as `{severity}`"
        ));
    }
    if phase(code).is_none() {
        report.violation(format!(
            "{REGISTRY}:{num}: `{code}` is outside the reserved ranges in LANGUAGE.md §8"
        ));
    }
}

fn valid_format(code: &str) -> bool {
    let bytes = code.as_bytes();
    bytes.len() == 5
        && matches!(bytes[0], b'E' | b'W' | b'L')
        && bytes[1..].iter().all(u8::is_ascii_digit)
}

/// The phase a code's range is reserved for, per `LANGUAGE.md §8`.
fn phase(code: &str) -> Option<&'static str> {
    match (code.as_bytes()[0], code.as_bytes()[1]) {
        (b'E', b'0') => Some("lexical"),
        (b'E', b'1') => Some("syntax"),
        (b'E', b'2') => Some("names"),
        (b'E', b'3') => Some("types"),
        (b'E', b'4') => Some("control flow"),
        (b'E', b'5') => Some("story graph"),
        (b'E', b'6') => Some("internal"),
        (b'E', b'7') => Some("build/assets"),
        (b'W', b'1') => Some("style lints"),
        (b'W', b'4') => Some("suspicious code"),
        (b'W', b'7') => Some("build lints"),
        _ => None,
    }
}

/// Whether a path is a test file.
///
/// Both layouts are in use: a `tests/` directory, and a crate that keeps its tests in
/// `tests.rs`. Recognising only the first made a code look untested when it was not.
fn is_test_path(rel: &str) -> bool {
    rel.split('/')
        .any(|part| part == "tests" || part.starts_with("tests.") || part.ends_with("_tests.rs"))
}

/// Whether any test file mentions this code.
fn referenced_in_test(root: &Path, code: &str) -> bool {
    for dir in ["crates", "tests"] {
        let base = root.join(dir);
        if !base.exists() {
            continue;
        }
        for path in scan::walk(&base, &["rs"]) {
            let rel = path.to_string_lossy().replace('\\', "/");
            if !is_test_path(&rel) {
                continue;
            }
            if scan::read(&path).is_some_and(|t| t.contains(code)) {
                return true;
            }
        }
    }
    false
}
