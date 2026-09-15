//! The canonical form, over every `.vela` file in the repository.
//!
//! This is what makes "canonical form" a fact rather than a claim (`TOOLING.md §3`): the fixture
//! set is the language's surface, so a formatter that is a fixed point on all of it, that re-parses
//! to a file with no complaints, and that refuses exactly what does not parse is a formatter whose
//! rules are pinned by the code that uses them.
//!
//! What it does *not* prove is that formatting preserves *meaning* — that needs a semantic oracle,
//! and the one this repository has is MIR, which `crates/vela-mir/tests/format_equivalence.rs`
//! compares. Between the two, a formatting change that alters a program is a failing test rather
//! than a review somebody has to notice.

use std::fs;
use std::path::{Path, PathBuf};

use vela_diag::Severity;
use vela_span::FileId;
use vela_syntax::{format, parse};

/// The repository root, from this crate's manifest directory.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the repository root")
        .to_path_buf()
}

/// Every `.vela` file under `dir`, skipping build output.
fn collect(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if matches!(name.as_str(), "dist" | "target" | ".git") {
                continue;
            }
            collect(&path, found);
        } else if name.ends_with(".vela") {
            found.push(path);
        }
    }
}

/// Every fixture, sorted so a failure names the same file twice.
fn fixtures() -> Vec<PathBuf> {
    let mut found = Vec::new();
    collect(&root(), &mut found);
    found.sort();
    assert!(
        found.len() > 40,
        "the walk found {} files; it is looking in the wrong place",
        found.len()
    );
    found
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// Whether a file has a syntax error, which is the only reason to refuse one.
fn is_broken(src: &str) -> bool {
    parse(FileId::from_raw(0), src)
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity() == Severity::Error)
}

#[test]
fn formatting_is_refused_exactly_when_a_file_does_not_parse() {
    for path in fixtures() {
        let src = read(&path);
        let result = format(FileId::from_raw(0), &src);

        assert_eq!(
            result.is_err(),
            is_broken(&src),
            "{} was {} but {} parse",
            path.display(),
            if result.is_err() {
                "refused"
            } else {
                "formatted"
            },
            if is_broken(&src) { "does not" } else { "does" }
        );
    }
}

/// The property a formatter is judged by: running it twice changes nothing the second time.
#[test]
fn every_fixture_formats_to_a_fixed_point() {
    let mut formatted = 0;

    for path in fixtures() {
        let src = read(&path);
        let Ok(once) = format(FileId::from_raw(0), &src) else {
            continue;
        };
        let twice = format(FileId::from_raw(0), &once)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));

        assert_eq!(
            once,
            twice,
            "{} is not a fixed point:\n--- once ---\n{once}\n--- twice ---\n{twice}",
            path.display()
        );
        formatted += 1;
    }

    assert!(formatted > 40, "only {formatted} fixtures were formatted");
}

/// The canonical form is a `.vela` file: it parses, and it says nothing about itself.
#[test]
fn the_canonical_form_parses_without_complaint() {
    for path in fixtures() {
        let src = read(&path);
        let Ok(formatted) = format(FileId::from_raw(0), &src) else {
            continue;
        };

        // Errors, not every diagnostic: a lint is not a complaint about syntax, and a file
        // containing a `# fmt: off` pragma is *expected* to draw one (`W4011`).
        let reparsed = parse(FileId::from_raw(0), &formatted);
        let errors: Vec<&str> = reparsed
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity() == Severity::Error)
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();
        assert!(
            errors.is_empty(),
            "{} produced a file that does not parse: {errors:?}\n{formatted}",
            path.display()
        );
    }
}

/// A formatter that dropped a comment would be deleting something its author wrote, and it is the
/// failure that looks like success — so it gets a test of its own, over every fixture.
#[test]
fn no_comment_is_lost() {
    for path in fixtures() {
        let src = read(&path);
        let Ok(formatted) = format(FileId::from_raw(0), &src) else {
            continue;
        };

        let before = parse(FileId::from_raw(0), &src).program.comments;
        let after = parse(FileId::from_raw(0), &formatted).program.comments;
        let before: Vec<&str> = before.iter().map(|c| c.text.as_str()).collect();
        let after: Vec<&str> = after.iter().map(|c| c.text.as_str()).collect();

        assert_eq!(
            before,
            after,
            "{} lost or gained a comment:\n{formatted}",
            path.display()
        );
    }
}
