//! `vela fmt`, through the command line a person uses.
//!
//! The rules themselves are `vela-syntax`'s, pinned there over every fixture in the repository. What
//! is pinned here is the part that touches a filesystem: that `--check` changes nothing, that a
//! plain run changes only what is not canonical, and that a file with a syntax error is left alone
//! rather than rewritten from a guess.

use super::support::{cli, temp_project};

/// A program indented two spaces per level, which is legal and not canonical.
const LOOSE: &str = "\
label start:
  scene bg.room
  menu \"Pick\":
    \"Go\":
      \"On.\"
  return
";

/// The same program at four spaces — what [`LOOSE`] is supposed to become.
const TIDY: &str = "\
label start:
    scene bg.room
    menu \"Pick\":
        \"Go\":
            \"On.\"
    return
";

#[test]
fn fmt_rewrites_a_file_into_its_canonical_form() {
    let project = temp_project("fmt", LOOSE);

    let (code, out) = cli(&["fmt", &project.to_string_lossy()]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("reformatted main.vela"), "{out}");
    assert_eq!(
        std::fs::read_to_string(project.join("src/main.vela")).expect("read"),
        TIDY
    );

    // And the verdict `--check` gives is what CI runs.
    let (code, out) = cli(&["fmt", "--check", &project.to_string_lossy()]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("already canonical"), "{out}");
}

#[test]
fn fmt_check_reports_without_writing() {
    let project = temp_project("fmt-check", LOOSE);

    let (code, out) = cli(&["fmt", "--check", &project.to_string_lossy()]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("would reformat main.vela"), "{out}");
    assert_eq!(
        std::fs::read_to_string(project.join("src/main.vela")).expect("read"),
        LOOSE,
        "`--check` wrote to the file"
    );
}

/// A formatter that reformatted a file it could not parse would be replacing the unparsable part
/// with nothing. The file is left exactly as it is, and the parser's own diagnostic is what the
/// reader sees.
#[test]
fn fmt_refuses_a_file_that_does_not_parse() {
    let project = temp_project("fmt-broken", "label start:\n    if\n");
    let before = std::fs::read_to_string(project.join("src/main.vela")).expect("read");

    let (code, out) = cli(&["fmt", &project.to_string_lossy()]);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("E1"),
        "the parse error should be reported: {out}"
    );

    assert_eq!(
        std::fs::read_to_string(project.join("src/main.vela")).expect("read"),
        before,
        "a refused file was written anyway"
    );
}

#[test]
fn fmt_says_so_when_there_is_nothing_to_do() {
    let project = temp_project("fmt-clean", TIDY);

    let (code, out) = cli(&["fmt", &project.to_string_lossy()]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("1 file(s) already canonical"), "{out}");
}

/// The point of a canonical form: two files that were laid out differently converge, so a diff
/// shows what changed rather than how someone typed it.
#[test]
fn two_layouts_of_one_program_converge() {
    let loose = temp_project("fmt-loose", LOOSE);
    let tidy = temp_project("fmt-tidy", TIDY);

    let (code, _) = cli(&["fmt", &loose.to_string_lossy()]);
    assert_eq!(code, 0);
    let (code, _) = cli(&["fmt", &tidy.to_string_lossy()]);
    assert_eq!(code, 0);

    assert_eq!(
        std::fs::read_to_string(loose.join("src/main.vela")).expect("read"),
        std::fs::read_to_string(tidy.join("src/main.vela")).expect("read"),
    );
}

/// `--diff` says how the file would change, and — like `--check`, but for a different reader —
/// changes nothing.
#[test]
fn fmt_diff_shows_the_change_without_making_it() {
    let project = temp_project("fmt-diff", LOOSE);

    let (code, out) = cli(&["fmt", "--diff", &project.to_string_lossy()]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--- main.vela"), "{out}");
    assert!(out.contains("-  scene bg.room"), "{out}");
    assert!(out.contains("+    scene bg.room"), "{out}");
    assert!(
        !out.contains("would reformat"),
        "the diff names the file itself: {out}"
    );

    assert_eq!(
        std::fs::read_to_string(project.join("src/main.vela")).expect("read"),
        LOOSE,
        "`--diff` wrote to the file"
    );
}

/// The two flags answer different questions about the same finding, so asking both at once is a
/// usage error rather than a choice the command makes for the caller.
#[test]
fn fmt_refuses_check_and_diff_together() {
    let project = temp_project("fmt-both", LOOSE);
    let (code, out) = cli(&["fmt", "--check", "--diff", &project.to_string_lossy()]);

    assert_eq!(code, 2, "{out}");
    assert!(out.contains("pick one"), "{out}");
}

/// A `# fmt: off` region keeps its own layout *relative to its first line*.
///
/// The first line moves to the indent the block calls for, and everything under it moves by the same
/// amount. Moving only the first line would be worse than reformatting the region: indentation is
/// structure here, so a nested block inside a region would end up nested differently than its author
/// wrote it.
#[test]
fn a_pragma_region_keeps_its_own_layout() {
    let project = temp_project(
        "fmt-pragma",
        "label start:\n  # fmt: off\n  if flag:\n      var   x   =   1\n  # fmt: on\n  return\n",
    );

    let (code, out) = cli(&["fmt", &project.to_string_lossy()]);
    assert_eq!(code, 0, "{out}");

    let formatted = std::fs::read_to_string(project.join("src/main.vela")).expect("read");
    // The block's statements are canonical, and the region is reproduced as written — including the
    // *extra* indentation its author gave that one line.
    assert!(formatted.contains("\n    # fmt: off\n"), "{formatted}");
    assert!(formatted.contains("\n    if flag:\n"), "{formatted}");
    assert!(
        formatted.contains("\n        var   x   =   1\n"),
        "{formatted}"
    );
    assert!(formatted.contains("\n    # fmt: on\n"), "{formatted}");
    assert!(formatted.contains("\n    return\n"), "{formatted}");

    // And it is stable: a second run finds nothing to do, which is what makes the region a region.
    let (code, out) = cli(&["fmt", "--check", &project.to_string_lossy()]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("already canonical"), "{out}");
}

/// A comment is the one thing a formatter may not invent or discard.
#[test]
fn fmt_keeps_comments() {
    let project = temp_project(
        "fmt-comments",
        "# What this label is for.\nlabel start:\n  \"Hi.\"  # and why\n  return\n",
    );

    let (code, out) = cli(&["fmt", &project.to_string_lossy()]);
    assert_eq!(code, 0, "{out}");

    let formatted = std::fs::read_to_string(project.join("src/main.vela")).expect("read");
    assert!(
        formatted.contains("# What this label is for."),
        "{formatted}"
    );
    assert!(formatted.contains("# and why"), "{formatted}");
}
