//! Golden tests for text layout.
//!
//! This is the exit criterion that *"a fixed string lays out byte-identically across the CI
//! matrix"*. Pinning the whole layout rather than a few numbers is the point: a wrapping
//! bug usually shows up as one line shifting by a pixel, and a test that asserts a line
//! count cannot see it.
//!
//! The face is vendored in `assets/fonts`, not read from a system path, because a golden
//! against whatever font the machine happens to have is not a golden — it is a
//! machine-specific assertion wearing a golden's clothes.
//!
//! `cargo xtask bless` regenerates every `.expected`. CI never blesses.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use vela_text::{Font, TextEngine};

/// The corpus directory.
fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/layout")
}

/// Whether to rewrite the goldens instead of comparing them.
fn blessing() -> bool {
    std::env::var_os("VELA_BLESS").is_some()
}

/// The vendored face.
fn font() -> Font {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts/LiberationSans-Regular.ttf");
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        panic!("cannot read {}: {error}", path.display());
    });
    Font::from_bytes(bytes, 0).expect("the vendored font must load")
}

/// A fixture: a `size width` header, then the text.
struct Fixture {
    size: f32,
    width: Option<f32>,
    text: String,
}

fn parse(source: &str) -> Fixture {
    let (header, body) = source
        .split_once('\n')
        .expect("a fixture needs a header line");
    let mut parts = header.split_whitespace();
    let size: f32 = parts
        .next()
        .expect("a size")
        .parse()
        .expect("a numeric size");
    let width = parts.next().and_then(|value| {
        if value == "-" {
            None
        } else {
            Some(value.parse().expect("a numeric width"))
        }
    });
    Fixture {
        size,
        width,
        // The fixture file's own final newline is formatting, not content. A paragraph that
        // ends mid-file is still a paragraph; expecting authors to strip it would make every
        // fixture differ by a line for no reason.
        text: body.strip_suffix('\n').unwrap_or(body).to_string(),
    }
}

/// Renders a layout as text, with positions pinned to three decimals.
///
/// Three decimals is a thousandth of a pixel: finer than any difference a reader could see,
/// coarser than floating-point noise. A golden that printed full `f32` precision would fail
/// on a different libm rounding and tell you nothing.
fn render(engine: &mut TextEngine, name: &str, fixture: &Fixture) -> String {
    let mut out = String::new();
    let layout = engine
        .layout(name, fixture.size, &fixture.text, fixture.width)
        .expect("the fixture used an unregistered font");

    let _ = writeln!(out, "size {}", fixture.size);
    match fixture.width {
        Some(width) => {
            let _ = writeln!(out, "wrap {width}");
        }
        None => out.push_str("wrap none\n"),
    }
    let _ = writeln!(
        out,
        "metrics ascent={:.3} descent={:.3} leading={:.3}",
        layout.metrics.ascent, layout.metrics.descent, layout.metrics.leading
    );
    let _ = writeln!(
        out,
        "size {:.3}x{:.3} lines={}",
        layout.width,
        layout.height,
        layout.lines.len()
    );

    for (index, line) in layout.lines.iter().enumerate() {
        let _ = writeln!(
            out,
            "line {index} width={:.3} baseline={:.3} range={}..{} glyphs={}",
            line.width,
            line.baseline,
            line.range.start,
            line.range.end,
            line.glyphs.len()
        );
        for glyph in &line.glyphs {
            let _ = writeln!(
                out,
                "  {} at {:.3} adv {:.3}",
                glyph.id, glyph.x, glyph.advance
            );
        }
    }
    out
}

#[test]
fn goldens_match() {
    let mut engine = TextEngine::new();
    engine.add_font("LiberationSans", font());

    let mut differences = Vec::new();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(corpus())
        .expect("the corpus directory must exist")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "txt"))
        .collect();
    entries.sort();

    for path in entries {
        let source = std::fs::read_to_string(&path).expect("read fixture");
        let fixture = parse(&source);
        let actual = render(&mut engine, "LiberationSans", &fixture);
        let expected_path = path.with_extension("expected");

        if blessing() {
            std::fs::write(&expected_path, &actual).expect("write golden");
            continue;
        }

        let expected = std::fs::read_to_string(&expected_path).unwrap_or_default();
        if actual != expected {
            differences.push(format!(
                "{}\n--- expected ---\n{expected}--- actual ---\n{actual}",
                path.display()
            ));
        }
    }

    assert!(
        differences.is_empty(),
        "{} golden(s) differ — run `cargo xtask bless`, then review the diff\n\n{}",
        differences.len(),
        differences.join("\n")
    );
}
