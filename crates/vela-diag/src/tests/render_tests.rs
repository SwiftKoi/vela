use vela_span::{SourceMap, Span};

use crate::{Code, Diagnostic, render};

const SAMPLE: &str = "label start:\n    jump forest.clearring\n";

fn code(name: &str) -> Code {
    Code::new(name).unwrap_or_else(|| panic!("{name} should be registered"))
}

#[test]
fn renders_an_error_with_location_caret_help_and_note() {
    let mut sources = SourceMap::new();
    let id = sources.add("chapters/start.vela", SAMPLE);

    let text = "forest.clearring";
    let start = SAMPLE.find(text).unwrap() as u32;
    let span = Span::new(id, start, start + text.len() as u32);

    let diagnostic = Diagnostic::new(
        code("E5003"),
        "undefined label `forest.clearring`",
        span,
        "no label `clearring` in module `forest`",
    )
    .with_help("did you mean `forest.clearing`?")
    .with_note("labels in `forest`: clearing, river, camp")
    .with_suggestion(span, "forest.clearing");

    // Gutter is 1 wide (line 2), the caret sits 9 display columns in, and the label
    // is underlined for its full width.
    let expected = [
        "error[E5003]: undefined label `forest.clearring`".to_string(),
        " --> chapters/start.vela:2:10".to_string(),
        "  |".to_string(),
        "2 |     jump forest.clearring".to_string(),
        format!(
            "  |{}{} no label `clearring` in module `forest`",
            " ".repeat(10),
            "^".repeat(text.len())
        ),
        "  |".to_string(),
        "  = help: did you mean `forest.clearing`?".to_string(),
        "  = note: labels in `forest`: clearing, river, camp".to_string(),
        String::new(),
    ]
    .join("\n");

    assert_eq!(render(&diagnostic, &sources), expected);
}

#[test]
fn a_warning_renders_as_a_warning() {
    let mut sources = SourceMap::new();
    let id = sources.add("chapters/start.vela", "label unused:\n    return\n");
    let span = Span::new(id, 0, 5);

    let diagnostic = Diagnostic::new(
        code("W4002"),
        "unreachable label `unused`",
        span,
        "no path reaches this",
    );
    let rendered = render(&diagnostic, &sources);

    assert!(rendered.starts_with("warning[W4002]: unreachable label `unused`\n"));
}

#[test]
fn a_suggestion_without_help_text_still_renders_a_fix() {
    let mut sources = SourceMap::new();
    let id = sources.add("main.vela", "\u{feff}label start:\n");
    let span = Span::new(id, 0, 3);

    let diagnostic = Diagnostic::new(
        code("E0001"),
        "byte order mark is not permitted",
        span,
        "remove this",
    )
    .with_suggestion(span, "");

    let rendered = render(&diagnostic, &sources);
    assert!(rendered.contains("= help: replace with ``"), "{rendered}");
}

#[test]
fn tabs_are_measured_not_counted_when_placing_carets() {
    let mut sources = SourceMap::new();
    // A tab before the target must advance the caret by four columns, not one.
    let id = sources.add("tabs.vela", "label start:\n\tjump other\n");
    let start = "\t".len() as u32;
    let span = Span::new(id, 13 + start, 13 + start + 5);

    let diagnostic = Diagnostic::new(code("E0003"), "tab used for indentation", span, "here");
    let rendered = render(&diagnostic, &sources);

    let caret_line = rendered
        .lines()
        .find(|l| l.contains('^'))
        .expect("a caret line");
    // "  | " + 4 columns of tab + "jump" underlined.
    assert!(caret_line.starts_with("  |     ^^^^^"), "{caret_line:?}");
}

#[test]
fn a_missing_source_file_degrades_instead_of_panicking() {
    let sources = SourceMap::new();
    let span = Span::new(vela_span::FileId::from_raw(9), 0, 1);
    let diagnostic = Diagnostic::new(code("E5003"), "undefined label", span, "here");

    let rendered = render(&diagnostic, &sources);
    assert!(
        rendered.contains("source file is not available"),
        "{rendered}"
    );
}

#[test]
fn secondary_labels_on_another_line_are_reported_by_location() {
    let mut sources = SourceMap::new();
    let source = "label a:\n    return\nlabel b:\n    return\n";
    let id = sources.add("main.vela", source);
    let primary = Span::new(id, 0, 5);
    let other = source.find("label b").unwrap() as u32;

    let diagnostic = Diagnostic::new(code("E5003"), "undefined label", primary, "jump target")
        .with_secondary(Span::new(id, other, other + 5), "declared here");

    let rendered = render(&diagnostic, &sources);
    assert!(
        rendered.contains("= at main.vela:3: declared here"),
        "{rendered}"
    );
}
