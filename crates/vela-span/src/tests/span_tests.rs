use crate::{FileId, Span};

fn span(start: u32, end: u32) -> Span {
    Span::new(FileId::from_raw(0), start, end)
}

#[test]
fn len_and_emptiness() {
    assert_eq!(span(3, 7).len(), 4);
    assert!(!span(3, 7).is_empty());
    assert!(span(3, 3).is_empty());
    // A reversed span is degenerate rather than a panic: spans come from parsing,
    // and a bug there must not take down the compiler.
    assert!(span(7, 3).is_empty());
    assert_eq!(span(7, 3).len(), 0);
}

#[test]
fn to_joins_covering_both() {
    assert_eq!(span(2, 5).to(span(10, 12)), span(2, 12));
    // Order does not matter, and overlap is fine.
    assert_eq!(span(10, 12).to(span(2, 5)), span(2, 12));
    assert_eq!(span(2, 10).to(span(5, 6)), span(2, 10));
}

#[test]
fn offset_shifts_both_ends() {
    assert_eq!(span(2, 5).offset(10), span(12, 15));
    assert_eq!(span(2, 5).offset(0), span(2, 5));
}

#[test]
fn accessors_round_trip() {
    let s = span(4, 9);
    assert_eq!(s.file(), FileId::from_raw(0));
    assert_eq!(s.start(), 4);
    assert_eq!(s.end(), 9);
}

#[test]
#[should_panic(expected = "cannot join spans across files")]
fn joining_across_files_is_a_bug_not_a_bug_report() {
    let a = Span::new(FileId::from_raw(0), 0, 1);
    let b = Span::new(FileId::from_raw(1), 0, 1);
    let _ = a.to(b);
}
