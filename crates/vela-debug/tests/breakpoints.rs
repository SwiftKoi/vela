//! Which sites a breakpoint matches.

use vela_debug::Breakpoints;
use vela_span::FileId;

/// A line breakpoint is per file, and setting the lines of a file replaces them.
#[test]
fn line_breakpoints_are_per_file_and_replaced() {
    let mut breakpoints = Breakpoints::new();
    let file = FileId::from_raw(0);

    breakpoints.set_lines(file, [3, 7]);
    assert!(breakpoints.hits_line(file, 3));
    assert!(breakpoints.hits_line(file, 7));
    assert!(!breakpoints.hits_line(file, 4));
    assert!(!breakpoints.hits_line(FileId::from_raw(1), 3));

    breakpoints.set_lines(file, [9]);
    assert!(
        !breakpoints.hits_line(file, 3),
        "setting replaces, not adds"
    );
    assert!(breakpoints.hits_line(file, 9));

    breakpoints.set_lines(file, []);
    assert!(!breakpoints.hits_line(file, 9), "an empty list clears");
    assert!(breakpoints.is_empty());
}

/// A label breakpoint matches the name as an author wrote it and its qualified form.
#[test]
fn label_breakpoints_match_qualified_and_bare_names() {
    let mut breakpoints = Breakpoints::new();
    breakpoints.set_labels(["later".to_string()]);

    assert!(breakpoints.hits_label("later"));
    assert!(breakpoints.hits_label("main.later"));
    assert!(breakpoints.hits_label("chapters.forest.later"));
    assert!(!breakpoints.hits_label("main.start"));
    // Not a suffix of a *longer name*: `ater` must not match `later`.
    assert!(!breakpoints.hits_label("main.ater"));
}
