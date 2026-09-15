//! Golden tests for diagnostic rendering.
//!
//! The goldens pin *rendering*: which span is underlined, how wide the caret is, how the
//! gutter lines up, what the message says. Tree *shape* is pinned by the unit tests,
//! which assert on specific nesting. Those are different jobs, and dividing them keeps
//! each golden small enough to read in a diff.
//!
//! `cargo xtask bless` regenerates every `.expected` from the current output. CI never
//! blesses: a golden that changes without review is a regression that was accepted
//! silently.

use std::fs;
use std::path::{Path, PathBuf};

use vela_diag::render;
use vela_span::SourceMap;

/// Whether to rewrite goldens rather than compare against them.
fn blessing() -> bool {
    std::env::var_os("VELA_BLESS").is_some()
}

/// The directory holding `<name>.vela` inputs and their `<name>.expected` output.
fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/parse")
}

#[test]
fn goldens_match() {
    let dir = golden_dir();
    let entries =
        fs::read_dir(&dir).unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));

    let mut inputs: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "vela"))
        .collect();
    inputs.sort();
    assert!(!inputs.is_empty(), "no golden inputs in {}", dir.display());

    let mut differences = Vec::new();
    for input in &inputs {
        let actual = render_file(input);
        let expected_path = input.with_extension("expected");

        if blessing() {
            fs::write(&expected_path, &actual).expect("write golden");
            continue;
        }

        let expected = fs::read_to_string(&expected_path).unwrap_or_default();
        if actual != expected {
            differences.push(format!(
                "{}\n--- expected ---\n{expected}--- actual ---\n{actual}",
                input.display()
            ));
        }
    }

    assert!(
        differences.is_empty(),
        "{} golden(s) differ — run `cargo xtask bless`, then review the diff\n\n{}",
        differences.len(),
        differences.join("\n")
    );
}

/// Parses one golden input and renders its diagnostics.
///
/// The file's own name is used in the output, not its path, so a golden does not depend
/// on where the checkout lives.
fn render_file(path: &Path) -> String {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let mut sources = SourceMap::new();
    let id = sources.add(name, &text);
    let result = vela_syntax::parse(id, &text);

    let mut out = String::new();
    for diagnostic in &result.diagnostics {
        out.push_str(&render(diagnostic, &sources));
    }
    out
}

/// Every lexical and syntactic code must have a golden input.
///
/// This is what makes "each `E0xxx` and `E1xxx` code has a golden" a fact rather than a
/// claim: adding a code without an input that triggers it fails here. Codes from later
/// phases are out of scope — a name-resolution error cannot be triggered by parsing
/// alone.
#[test]
fn every_lexical_and_syntactic_code_has_a_golden() {
    let registry = Path::new(env!("CARGO_MANIFEST_DIR")).join("../vela-diag/codes.txt");
    let text = fs::read_to_string(&registry)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", registry.display()));

    let mut missing = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(code) = line.split('|').next() else {
            continue;
        };
        if !code.starts_with("E0") && !code.starts_with("E1") {
            continue;
        }
        if !golden_dir()
            .join(format!("diag_{}.vela", code.to_lowercase()))
            .is_file()
        {
            missing.push(code.to_string());
        }
    }

    assert!(
        missing.is_empty(),
        "no golden input for: {}",
        missing.join(", ")
    );
}
