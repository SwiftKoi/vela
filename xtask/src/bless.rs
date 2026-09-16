//! `cargo xtask bless` — regenerate golden files.
//!
//! Blessing runs the golden tests with `VELA_BLESS` set, so the test itself remains the
//! single definition of the golden format. Duplicating that logic here would let the
//! two drift, and a blesser that disagrees with its test is worse than none.

use std::process::Command;

/// The golden test binaries, as `(package, test target)`.
const GOLDEN_TESTS: &[(&str, &str)] = &[
    ("vela-syntax", "parse_golden"),
    ("vela-mir", "mir_golden"),
    ("vela-text", "layout_golden"),
    ("vela-replay", "corpus"),
    ("vela-assets", "assets_golden"),
    ("vela-cli", "analyze_golden"),
    ("vela-cli", "doc_golden"),
];

/// Regenerates every golden, returning whether all of them succeeded.
pub fn run() -> bool {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut ok = true;

    for (package, test) in GOLDEN_TESTS {
        println!("  bless {package} --test {test}");

        let status = Command::new(&cargo)
            .args(["test", "-p", package, "--test", test])
            .env("VELA_BLESS", "1")
            .status();

        match status {
            Ok(status) if status.success() => {}
            _ => {
                eprintln!("  FAIL  {package} --test {test}");
                ok = false;
            }
        }
    }

    if ok {
        println!("\ngoldens regenerated — review the diff before committing");
    }
    ok
}
