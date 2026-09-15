use crate::{SourceMap, Span};

const SAMPLE: &str = "label start:\n    \"Hello.\"\n    jump other\n";

#[test]
fn line_text_excludes_the_newline() {
    let mut map = SourceMap::new();
    let id = map.add("main.vela", SAMPLE);
    let file = map.file(id);

    assert_eq!(file.line_text(0), Some("label start:"));
    assert_eq!(file.line_text(1), Some("    \"Hello.\""));
    assert_eq!(file.line_text(2), Some("    jump other"));
    // The trailing newline creates a final empty line.
    assert_eq!(file.line_count(), 4);
    assert_eq!(file.line_text(3), Some(""));
    assert_eq!(file.line_text(4), None);
}

#[test]
fn crlf_terminators_are_trimmed() {
    let mut map = SourceMap::new();
    let id = map.add("win.vela", "a\r\nb\r\n");
    let file = map.file(id);

    assert_eq!(file.line_text(0), Some("a"));
    assert_eq!(file.line_text(1), Some("b"));
}

#[test]
fn line_col_maps_offsets_to_positions() {
    let mut map = SourceMap::new();
    let id = map.add("main.vela", SAMPLE);
    let file = map.file(id);

    // Start of the file.
    assert_eq!((file.line_col(0).line, file.line_col(0).col), (0, 0));
    // The `"` on line 1, which is 4 spaces in.
    let quote = SAMPLE.find("\"Hello").unwrap() as u32;
    let lc = file.line_col(quote);
    assert_eq!((lc.line, lc.col), (1, 4));
    // The `j` of `jump`, on line 2.
    let jump = SAMPLE.find("jump").unwrap() as u32;
    let lc = file.line_col(jump);
    assert_eq!((lc.line, lc.col), (2, 4));
}

#[test]
fn line_col_clamps_instead_of_panicking() {
    let mut map = SourceMap::new();
    let id = map.add("main.vela", "abc");
    let file = map.file(id);

    // A span that runs past the end of a file must not panic: diagnostics are often
    // produced for truncated input.
    let lc = file.line_col(9_999);
    assert_eq!(lc.line, 0);
    assert_eq!(lc.col, 9_999);
}

#[test]
fn text_extracts_the_span_bytes() {
    let mut map = SourceMap::new();
    let id = map.add("main.vela", SAMPLE);

    let start = SAMPLE.find("\"Hello.\"").unwrap() as u32;
    let span = Span::new(id, start, start + 8);
    assert_eq!(map.text(span), "\"Hello.\"");
}

#[test]
fn out_of_range_spans_degrade_to_empty() {
    let mut map = SourceMap::new();
    let id = map.add("main.vela", SAMPLE);

    // Beyond the end of the text.
    assert_eq!(map.text(Span::new(id, 0, 9_999)), "");
    // An unknown file id is not in this map.
    assert_eq!(map.text(Span::new(crate::FileId::from_raw(7), 0, 1)), "");
    assert!(map.get(crate::FileId::from_raw(7)).is_none());
}

#[test]
fn line_col_of_a_span_uses_its_start() {
    let mut map = SourceMap::new();
    let id = map.add("main.vela", SAMPLE);
    let jump = SAMPLE.find("jump").unwrap() as u32;

    let lc = map.line_col(Span::new(id, jump, jump + 4)).unwrap();
    assert_eq!((lc.line, lc.col), (2, 4));
}
