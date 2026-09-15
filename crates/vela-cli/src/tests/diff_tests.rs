//! The unified diff `vela fmt --diff` prints.
//!
//! A diff is easy to write and easy to get subtly wrong: the hunk header's line numbers and counts
//! are what a patch tool reads, and a test that only looks for `-old`/`+new` would pass on output
//! that no tool could apply. So these check the headers too, including the two cases that are
//! special — a change at the first line, and an insertion where one side is empty.

use crate::diff::unified;

/// The body of a diff, without its `---`/`+++` header lines.
fn body(path: &str, before: &str, after: &str) -> Vec<String> {
    unified(path, before, after)
        .lines()
        .skip(2)
        .map(str::to_string)
        .collect()
}

#[test]
fn identical_text_is_no_diff_at_all() {
    assert!(unified("a.vela", "one\ntwo\n", "one\ntwo\n").is_empty());
}

#[test]
fn a_changed_line_shows_both_sides_with_context() {
    let before = "1\n2\n3\n4\n5\n6\n7\n8\n9\n";
    let after = "1\n2\n3\nfour\n5\n6\n7\n8\n9\n";
    let lines = body("a.vela", before, after);

    assert_eq!(
        lines,
        vec![
            "@@ -1,7 +1,7 @@",
            " 1",
            " 2",
            " 3",
            "-4",
            "+four",
            " 5",
            " 6",
            " 7",
        ]
    );
}

#[test]
fn the_header_names_the_file_on_both_sides() {
    let text = unified("src/main.vela", "a\n", "b\n");
    assert!(
        text.starts_with("--- src/main.vela\n+++ src/main.vela\n"),
        "{text}"
    );
}

/// The first line is the case with no context to show above it, and a header count is easy to get
/// wrong there.
#[test]
fn a_change_at_the_first_line_has_no_leading_context() {
    let lines = body("a.vela", "one\ntwo\nthree\n", "ONE\ntwo\nthree\n");

    assert_eq!(
        lines,
        vec!["@@ -1,3 +1,3 @@", "-one", "+ONE", " two", " three"]
    );
}

/// A pure insertion names the line *before* it, which is the convention every patch tool reads: at
/// the top of a file that is `-0,0`.
#[test]
fn an_insertion_into_an_empty_file_names_line_zero() {
    let lines = body("a.vela", "", "one\ntwo\n");

    assert_eq!(lines, vec!["@@ -0,0 +1,2 @@", "+one", "+two"]);
}

#[test]
fn a_deletion_leaves_the_other_side_empty() {
    let lines = body("a.vela", "one\ntwo\n", "");

    assert_eq!(lines, vec!["@@ -1,2 +0,0 @@", "-one", "-two"]);
}

/// Two changes far apart are two hunks; two changes closer than the context is wide are one, so the
/// same line is never printed twice.
#[test]
fn distant_changes_are_separate_hunks_and_near_ones_are_merged() {
    let far = body(
        "a.vela",
        "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n",
        "one\n2\n3\n4\n5\n6\n7\n8\n9\nten\n",
    );
    let heads: Vec<&String> = far.iter().filter(|line| line.starts_with("@@")).collect();
    assert_eq!(heads.len(), 2, "{far:?}");

    let near = body("a.vela", "1\n2\n3\n4\n5\n6\n", "one\n2\n3\n4\n5\nsix\n");
    let heads: Vec<&String> = near.iter().filter(|line| line.starts_with("@@")).collect();
    assert_eq!(heads.len(), 1, "{near:?}");
    // The context between them appears once.
    assert_eq!(
        near.iter().filter(|line| *line == " 4").count(),
        1,
        "{near:?}"
    );
}

/// A hunk of one line omits its count, as `patch` expects: `@@ -1 +1 @@`, not `@@ -1,1 +1,1 @@`.
///
/// A one-line hunk only happens at the edges of the file — context is printed whenever there is any —
/// so the example is a file that *is* one line.
#[test]
fn a_single_line_hunk_omits_its_count() {
    let lines = body("a.vela", "a\n", "b\n");
    assert_eq!(lines, vec!["@@ -1 +1 @@", "-a", "+b"]);
}
