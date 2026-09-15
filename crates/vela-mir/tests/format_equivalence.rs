//! Formatting must not change what a file *means*.
//!
//! `crates/vela-syntax/tests/format_roundtrip.rs` proves the canonical form is stable and parses.
//! Stability is not meaning: a printer that dropped a clause would be perfectly idempotent. The
//! oracle for meaning in this repository is MIR — the same printed program the golden corpus pins —
//! so this compares the program a file lowers to against the program its canonical form lowers to,
//! over every fixture.
//!
//! It lives here rather than in `vela-syntax` because the comparison needs a back end, and the rank
//! rule forbids the parser reaching up for one. A test is allowed to be the lowest layer that can
//! see both.

use std::fs;
use std::path::{Path, PathBuf};

use vela_hir::ModuleName;
use vela_span::{FileId, SourceMap};
use vela_types::Env;

/// The module name both sides are lowered under. The printer writes it, so it has to match.
const NAME: &str = "fixture";

/// The program a file lowers to, printed.
fn mir_of(src: &str) -> String {
    let mut sources = SourceMap::new();
    let id: FileId = sources.add(NAME, src);
    let parsed = vela_syntax::parse(id, src);
    let (env, _) = Env::build(&parsed.program);
    let lowered = vela_mir::lower(&ModuleName::new(NAME), &parsed.program, &env);
    vela_mir::print_module(&lowered.module)
}

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

#[test]
fn a_formatted_file_lowers_to_the_same_program() {
    let mut found = Vec::new();
    collect(&root(), &mut found);
    found.sort();
    assert!(found.len() > 40, "the walk found {} files", found.len());

    let mut compared = 0;
    for path in &found {
        let src = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let Ok(formatted) = vela_syntax::format(FileId::from_raw(0), &src) else {
            // A file that does not parse is not the formatter's to change; the syntax test covers
            // that it is refused.
            continue;
        };

        assert_eq!(
            mir_of(&src),
            mir_of(&formatted),
            "formatting changed what {} means:\n{formatted}",
            path.display()
        );
        compared += 1;
    }

    assert!(compared > 40, "only {compared} fixtures were compared");
}
