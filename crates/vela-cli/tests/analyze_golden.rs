//! The analysis golden: a project in, a story graph out.
//!
//! `TOOLING.md §7` asks for two things of this report and this test is both of them. It must be
//! **deterministic** — "`vela analyze --format json` can be tracked as a metric over time in CI, our
//! unreachable-label count went up by three this week" — so every case here analyzes twice and fails if
//! the two runs differ. And it must be **diffable**, so each case is also compared against a committed
//! file: an unintended change to the graph shows up as a review, and an intended one is blessed.
//!
//! `cargo xtask bless` regenerates the files. CI never blesses.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Whether to rewrite goldens rather than compare against them.
fn blessing() -> bool {
    std::env::var_os("VELA_BLESS").is_some()
}

/// Where the committed reports live.
fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/analyze")
}

/// One case: a project to analyze, and the name its report is stored under.
struct Case {
    /// The name of the golden file, without its extension.
    name: &'static str,
    /// The project's root directory.
    project: PathBuf,
}

/// Every case, in a stable order.
fn cases() -> Vec<Case> {
    let mut cases = vec![Case {
        name: "standard",
        project: Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/standard"),
    }];

    cases.push(Case {
        name: "unreachable",
        project: fixture(),
    });
    cases
}

/// A project whose story has a label nothing can reach, which is what the report is for.
fn fixture() -> PathBuf {
    let base = std::env::temp_dir().join(format!("vela-analyze-golden-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(base.join("src").join("chapters")).expect("create the fixture");

    fs::write(
        base.join("vela.toml"),
        "schema = 1\n\n[project]\nname = \"analysis\"\nversion = \"0.1.0\"\nentry = \"main.start\"\n",
    )
    .expect("write the manifest");
    fs::write(
        base.join("src").join("main.vela"),
        "\
use chapters.road

label start:
    jump chapters.road.onward

# Nothing jumps here, and that is the report: `W4002` says so in `vela check`, and the graph below shows
# the label without an edge pointing at it.
label detour:
    \"Nobody comes this way.\"
    return
",
    )
    .expect("write the entry");
    fs::write(
        base.join("src").join("chapters").join("road.vela"),
        "\
use main

label onward:
    \"The road goes on.\"
    menu \"Which way?\":
        \"Back\":
            jump main.start
",
    )
    .expect("write the chapter");

    base
}

/// Runs `vela analyze --format json` and returns what it printed.
fn analyze(project: &Path) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_vela"))
        .args(["analyze"])
        .arg(project)
        .args(["--format", "json"])
        .output()
        .expect("run vela analyze");

    assert!(
        output.status.success(),
        "analyze failed on {}: {}",
        project.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf-8 output")
}

#[test]
fn goldens_match() {
    let dir = golden_dir();
    fs::create_dir_all(&dir).expect("create the golden directory");

    for case in cases() {
        let actual = analyze(&case.project);
        assert_eq!(
            actual,
            analyze(&case.project),
            "{}: two runs disagree, which is the one thing this report may not do",
            case.name
        );

        let path = dir.join(format!("{}.json", case.name));
        if blessing() {
            fs::write(&path, &actual).expect("bless");
            continue;
        }

        let expected = fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!(
                "cannot read {}: {error}\n  fix: cargo xtask bless",
                path.display()
            )
        });
        assert_eq!(
            actual, expected,
            "{}: the analysis changed\n  fix: review it, then `cargo xtask bless`",
            case.name
        );
    }
}
