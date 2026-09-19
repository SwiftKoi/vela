//! The reference is generated, and this is what keeps it generated.
//!
//! `TOOLING.md §9` says the docs read the same schemas the compiler does, so they "cannot drift from
//! behavior". That is a claim about a *build step*, and a claim with nothing checking it is a wish — so
//! this regenerates every page and compares it with what is committed. Adding a prop, an action, or a
//! diagnostic code and forgetting the reference fails here, with the fix in the message.
//!
//! It also checks the two interfaces agree: `vela doc` prints exactly what `vela doc --out` writes, so a
//! reader piping it into a file gets the same page as a build script does.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Whether to rewrite the reference rather than compare against it.
fn blessing() -> bool {
    std::env::var_os("VELA_BLESS").is_some()
}

/// Where the committed reference lives.
fn reference_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/reference")
}

/// The pages, which is the set `vela doc` generates.
const PAGES: [&str; 4] = ["widgets", "actions", "settings", "diagnostics"];

/// Runs `vela doc` and returns its standard output.
fn stdout(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_vela"))
        .arg("doc")
        .args(args)
        .output()
        .expect("run vela doc");

    assert!(
        output.status.success(),
        "vela doc {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf-8 output")
}

#[test]
fn the_reference_matches_the_schemas() {
    let generated = std::env::temp_dir().join(format!("vela-doc-{}", std::process::id()));
    let _ = fs::remove_dir_all(&generated);

    let out = generated.display().to_string();
    let code = Command::new(env!("CARGO_BIN_EXE_vela"))
        .args(["doc", "--out", &out])
        .status()
        .expect("run vela doc --out");
    assert!(code.success(), "vela doc --out {out} failed");

    let reference = reference_dir();
    if blessing() {
        fs::create_dir_all(&reference).expect("create docs/reference");
    }

    for page in PAGES {
        let written = fs::read_to_string(generated.join(format!("{page}.md")))
            .unwrap_or_else(|error| panic!("vela doc did not write {page}.md: {error}"));

        // The same page, asked for by name, on standard output.
        assert_eq!(
            stdout(&[page]),
            written,
            "{page}: printing the page and writing it disagree"
        );

        let path = reference.join(format!("{page}.md"));
        if blessing() {
            fs::write(&path, &written).expect("bless");
            continue;
        }

        let committed = fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!(
                "cannot read {}: {error}\n  fix: cargo xtask bless",
                path.display()
            )
        });
        assert_eq!(
            written, committed,
            "{page}: the schema changed and the reference did not\n  fix: review the diff, then `cargo \
             xtask bless`"
        );
    }
}
