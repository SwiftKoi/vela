//! Text layout and atlas behaviour.
//!
//! The goldens pin *what* a layout looks like; these pin *why*, in cases where a golden
//! would say "this changed" without saying "this broke".

use std::path::Path;

use vela_text::{Font, TextEngine};

fn font() -> Font {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts/LiberationSans-Regular.ttf");
    Font::from_bytes(std::fs::read(&path).expect("read font"), 0).expect("load font")
}

fn engine() -> TextEngine {
    let mut engine = TextEngine::new();
    engine.add_font("sans", font());
    engine
}

/// Wraps `text` at `width` and returns each line's text.
fn breaks_at(width: f32, text: &str) -> Vec<String> {
    let mut engine = engine();
    let layout = engine.layout("sans", 24.0, text, Some(width)).unwrap();
    layout
        .lines
        .iter()
        .map(|line| text[line.range.start as usize..line.range.end as usize].to_string())
        .collect()
}

/// **The property that caught the original bug.** Every line must fit the width it was
/// wrapped to — a wrapping routine that only checks overflow at the *next* break never
/// checks the final line at all, and produces one over-long line at the end.
#[test]
fn no_line_ever_exceeds_the_wrap_width() {
    let text = "The rain had stopped an hour ago, but the street still shone.";
    for width in [60.0, 90.0, 120.0, 200.0, 320.0, 480.0, 1000.0] {
        let mut engine = engine();
        let layout = engine.layout("sans", 24.0, text, Some(width)).unwrap();
        for (index, line) in layout.lines.iter().enumerate() {
            let content = &text[line.range.start as usize..line.range.end as usize];
            if line.width <= width {
                continue;
            }
            // Overflow is only allowed when the line could not have been broken: one word,
            // with nowhere to break inside it. Any other overflow is the bug this test is
            // for — a line that could have been split and was not.
            assert!(
                !content.trim().contains(' '),
                "{width}px: line {index} is {:.3}px wide and holds {content:?}",
                line.width,
            );
        }
    }
}

/// Lines partition the text: no byte is dropped and none is counted twice.
#[test]
fn the_lines_cover_the_whole_text() {
    let text = "The rain had stopped an hour ago, but the street still shone.";
    let lines = breaks_at(120.0, text);
    assert_eq!(lines.concat(), text);
}

#[test]
fn a_word_wider_than_the_line_takes_the_line_alone() {
    // Rather than looping or emitting empty lines: an unbreakable word has to go somewhere.
    let lines = breaks_at(60.0, "Supercalifragilisticexpialidocious");
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0], "Supercalifragilisticexpialidocious");
}

#[test]
fn a_wrapping_word_is_broken_where_it_no_longer_fits() {
    let lines = breaks_at(200.0, "The rain had stopped an hour ago");
    assert!(lines.len() > 1, "{lines:?}");
    for line in &lines {
        assert!(line.len() <= "The rain had stopped an hour ago".len());
    }
}

/// Shaping is doing more than mapping characters to glyphs, which a golden of positions
/// alone would not distinguish from a metrics table.
#[test]
fn kerning_narrows_a_kerned_pair() {
    let mut engine = engine();
    let layout = engine.layout("sans", 24.0, "TA", None).unwrap();
    let glyphs = &layout.lines[0].glyphs;
    assert_eq!(glyphs.len(), 2);

    // `T` alone, to compare against.
    let alone = engine.layout("sans", 24.0, "T", None).unwrap();
    let natural = alone.lines[0].glyphs[0].advance;
    assert!(
        glyphs[1].x < natural,
        "TA was not kerned: T advances {natural}, but A sits at {}",
        glyphs[1].x
    );
}

/// `ARCHITECTURE.md §8`: cached and invalidated by change. Identical input must not lay out
/// twice, and changed input must not reuse an entry.
#[test]
fn an_identical_layout_is_cached() {
    let mut engine = engine();
    let first = engine.layout("sans", 24.0, "Hello.", Some(200.0)).unwrap();
    let second = engine.layout("sans", 24.0, "Hello.", Some(200.0)).unwrap();
    assert!(std::rc::Rc::ptr_eq(&first, &second));
    assert_eq!(engine.cached_layouts(), 1);
}

#[test]
fn a_changed_layout_is_not_reused() {
    let mut engine = engine();
    engine.layout("sans", 24.0, "Hello.", Some(200.0)).unwrap();
    // Each of these differs in exactly one input, and each must miss the cache.
    engine.layout("sans", 25.0, "Hello.", Some(200.0)).unwrap();
    engine.layout("sans", 24.0, "Hello!", Some(200.0)).unwrap();
    engine.layout("sans", 24.0, "Hello.", Some(201.0)).unwrap();
    assert_eq!(engine.cached_layouts(), 4);
}

#[test]
fn an_unknown_font_yields_no_layout() {
    let mut engine = engine();
    assert!(engine.layout("serif", 24.0, "Hello.", None).is_none());
}

/// A glyph is rasterised once and uploaded once: asking twice must not re-rasterise, and
/// must not report the same rectangle as new a second time.
#[test]
fn a_glyph_is_rasterised_and_uploaded_once() {
    let mut engine = engine();
    let id = engine.layout("sans", 24.0, "A", None).unwrap().lines[0].glyphs[0].id;

    let first = engine.glyph("sans", 24.0, id).expect("A has a mask");
    assert_eq!(engine.atlas_mut().take_dirty().len(), 1);

    let second = engine.glyph("sans", 24.0, id).expect("A has a mask");
    assert_eq!(first, second);
    assert!(
        engine.atlas_mut().take_dirty().is_empty(),
        "a cached glyph was reported as new"
    );
    assert_eq!(engine.atlas().len(), 1);
}

/// A glyph with no pixels is remembered as such. A space is asked for on every line of every
/// frame, so re-rasterising it is exactly the per-frame cost the cache exists to avoid.
#[test]
fn a_blank_glyph_is_not_re_rasterised() {
    let mut engine = engine();
    let space = engine.layout("sans", 24.0, "a b", None).unwrap().lines[0].glyphs[1].id;

    assert!(engine.glyph("sans", 24.0, space).is_none());
    engine.atlas_mut().take_dirty();
    assert!(engine.glyph("sans", 24.0, space).is_none());
    assert!(engine.atlas_mut().take_dirty().is_empty());
}

/// Metrics are baseline-relative, so a line box is taller than the glyphs in it.
#[test]
fn metrics_are_baseline_relative() {
    let mut engine = engine();
    let layout = engine.layout("sans", 24.0, "x", None).unwrap();
    assert!(layout.metrics.ascent > 0.0);
    assert!(layout.metrics.descent > 0.0);
    assert!(layout.metrics.line_height() > layout.metrics.ascent);
    // The first baseline sits at the ascent, which is where text starts.
    assert!((layout.lines[0].baseline - layout.metrics.ascent).abs() < f32::EPSILON);
}

#[test]
fn lines_are_stacked_by_line_height() {
    let mut engine = engine();
    let wrapped = engine
        .layout("sans", 24.0, "one two three four five", Some(60.0))
        .unwrap();
    assert!(wrapped.lines.len() >= 2);
    let step = wrapped.lines[1].baseline - wrapped.lines[0].baseline;
    assert!((step - wrapped.metrics.line_height()).abs() < f32::EPSILON);
    assert!((wrapped.height - wrapped.lines.len() as f32 * step).abs() < 0.001);
}

#[test]
fn bytes_that_are_not_a_font_do_not_load() {
    assert!(Font::from_bytes(b"not a font".to_vec(), 0).is_none());
}
