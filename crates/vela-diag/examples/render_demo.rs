//! Prints a rendered diagnostic, so the output format can be seen and diffed without
//! reading a test assertion.
//!
//! Run with: `cargo run -p vela-diag --example render_demo`

use vela_diag::{Diagnostic, render};
use vela_span::{SourceMap, Span};

fn main() {
    let source = "\
label start:
    jump forest.clearring

label forest.clearing:
    return
";

    let mut sources = SourceMap::new();
    let file = sources.add("chapters/start.vela", source);

    let target = "forest.clearring";
    let start = source.find(target).expect("fixture contains the label") as u32;
    let span = Span::new(file, start, start + target.len() as u32);

    let diagnostic = Diagnostic::new(
        vela_diag::Code::new("E5003").expect("E5003 is registered"),
        "undefined label `forest.clearring`",
        span,
        "no label `clearring` in module `forest`",
    )
    .with_help("did you mean `forest.clearing`?")
    .with_note("labels in `forest`: clearing, river, camp")
    .with_suggestion(span, "forest.clearing");

    print!("{}", render(&diagnostic, &sources));
}
