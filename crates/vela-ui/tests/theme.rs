//! Theme tokens and the contrast lint.
//!
//! `W4009`'s value is that it says *the ratio is 2.8:1* rather than *this looks wrong* — a
//! number an author can act on. So the tests assert the number, not just the code.

use vela_span::FileId;
use vela_syntax::{Item, ThemeDecl, parse};
use vela_ui::check_contrast;
use vela_ui::theme::{AA_TEXT, Rgb, fonts, palette};

fn theme_of(source: &str) -> ThemeDecl {
    let parsed = parse(FileId::from_raw(0), source);
    assert!(parsed.diagnostics.is_empty(), "the fixture must parse");
    parsed
        .program
        .items
        .into_iter()
        .find_map(|item| match item {
            Item::Theme(theme) => Some(theme),
            _ => None,
        })
        .expect("expected a theme")
}

fn codes(diagnostics: &[vela_diag::Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|d| d.code.as_str().to_string())
        .collect()
}

/// The two extremes, which pin the scale: identical colours are 1:1, black on white is 21:1.
#[test]
fn the_contrast_scale_is_right() {
    let black = Rgb { r: 0, g: 0, b: 0 };
    let white = Rgb {
        r: 255,
        g: 255,
        b: 255,
    };
    assert!((black.contrast(&black) - 1.0).abs() < 0.001);
    assert!(
        (black.contrast(&white) - 21.0).abs() < 0.01,
        "{}",
        black.contrast(&white)
    );
    assert!(
        (black.contrast(&white) - white.contrast(&black)).abs() < 0.001,
        "and it is symmetric"
    );
}

/// The standard's own reference values, so the implementation is checked against the spec
/// rather than against itself.
#[test]
fn known_pairs_match_wcag() {
    let white = Rgb {
        r: 255,
        g: 255,
        b: 255,
    };
    // #767676 on white is the canonical "just passes AA" grey: 4.54:1.
    let grey = Rgb {
        r: 0x76,
        g: 0x76,
        b: 0x76,
    };
    let ratio = grey.contrast(&white);
    assert!((4.5..5.0).contains(&ratio), "{ratio}");

    // Pure blue on black is famously low for its apparent brightness: 2.44:1.
    let blue = Rgb { r: 0, g: 0, b: 255 };
    let black = Rgb { r: 0, g: 0, b: 0 };
    assert!(
        (blue.contrast(&black) - 2.44).abs() < 0.05,
        "{}",
        blue.contrast(&black)
    );
}

#[test]
fn a_theme_parses_its_colours() {
    let theme = theme_of(
        "theme dusk:\n    color bg = 0x10121a\n    color fg = 0xe6e6f0\n    space md = 8\n",
    );
    let palette = palette(&theme);
    assert_eq!(
        palette.get("bg"),
        Some(Rgb {
            r: 0x10,
            g: 0x12,
            b: 0x1a
        })
    );
    assert_eq!(
        palette.get("fg"),
        Some(Rgb {
            r: 0xe6,
            g: 0xe6,
            b: 0xf0
        })
    );
    // `space md = 8` is not a colour. The parser does not record hex-ness, so the palette tells
    // them apart by requiring a full six hex digits — see `theme.rs` for why that is a
    // consequence of a missing typed-token form rather than a design.
    assert_eq!(palette.colors.len(), 2);
}

/// The spec's own example passes, which is the first thing a lint should do.
#[test]
fn the_specs_example_is_clean() {
    let theme = theme_of(
        "theme dusk:\n    color bg = 0x10121a\n    color fg = 0xe6e6f0\n    color accent = 0x6ea8fe\n",
    );
    let diagnostics = check_contrast(&theme);
    assert!(diagnostics.is_empty(), "{:?}", codes(&diagnostics));
}

/// A low-contrast pair is reported, and the message carries the ratio.
#[test]
fn a_low_contrast_pair_is_reported_with_its_ratio() {
    // `#333333` on `#222222`: readable in a mockup, unreadable on a laptop in daylight.
    let theme = theme_of("theme murky:\n    color bg = 0x222222\n    color fg = 0x333333\n");
    let diagnostics = check_contrast(&theme);
    assert_eq!(codes(&diagnostics), vec!["W4009"]);

    let message = &diagnostics[0].message;
    assert!(
        message.contains("1.") && message.contains(":1"),
        "the ratio should be in the message: {message}"
    );
    assert!(
        message.contains("4.5"),
        "and so should the threshold: {message}"
    );
}

/// Both colours are named, so an author does not have to work out which is which.
#[test]
fn the_note_names_both_colours() {
    let theme = theme_of("theme murky:\n    color bg = 0x222222\n    color fg = 0x333333\n");
    let diagnostics = check_contrast(&theme);
    let note = diagnostics[0].notes.first().expect("a note").clone();
    assert!(note.contains("#222222"), "{note}");
    assert!(note.contains("#333333"), "{note}");
}

/// Every foreground is measured, not just the first.
#[test]
fn every_colour_is_checked() {
    let theme = theme_of(
        "theme murky:\n    color bg = 0x222222\n    color a = 0x333333\n    color b = 0x444444\n",
    );
    let diagnostics = check_contrast(&theme);
    assert_eq!(diagnostics.len(), 2, "{:?}", codes(&diagnostics));
}

/// A theme with no background has no pair to measure, and silence beats inventing one.
#[test]
fn a_theme_without_a_background_is_not_measured() {
    let theme = theme_of("theme partial:\n    color fg = 0x333333\n");
    assert!(check_contrast(&theme).is_empty());
}

/// A theme with no colours at all is clean rather than surprising.
#[test]
fn an_empty_theme_is_clean() {
    let theme = theme_of("theme bare:\n    space sm = 4\n");
    assert!(palette(&theme).is_empty());
    assert!(check_contrast(&theme).is_empty());
}

/// A value that is not six hex digits is left out rather than guessed at — a wrong colour
/// would make the lint measure something nobody wrote.
#[test]
fn a_non_colour_value_is_not_a_colour() {
    let theme = theme_of("theme odd:\n    color bg = 0x10121a\n    color huge = 0xFFFFFFF\n");
    assert_eq!(
        palette(&theme).colors.len(),
        1,
        "the oversized literal was taken"
    );
}

/// The threshold is a named constant, not a magic number in a comparison.
#[test]
fn the_threshold_is_stated() {
    assert!((AA_TEXT - 4.5).abs() < f64::EPSILON);
}

/// A theme reads its `font` tokens, told apart from the other settings by the type word.
///
/// `SCREENS.md §5`: `font body` and `space body` are one shape with one kind of value, and only the
/// word says which is which — the same reason a palette needs the word to read a colour.
#[test]
fn a_theme_parses_its_fonts() {
    let theme = theme_of(
        "theme dusk:\n    font ui = \"sans\"\n    font kanji = \"SourceHanSans\"\n    space md = 8\n",
    );
    let fonts = fonts(&theme);
    assert_eq!(fonts.get("ui"), Some("sans"));
    assert_eq!(fonts.get("kanji"), Some("SourceHanSans"));
    assert_eq!(fonts.get("md"), None, "a `space` token is not a font");
    assert_eq!(fonts.tokens.len(), 2);
}

/// A font token that is not a plain string is left out rather than guessed at — the same rule a
/// colour that is not six hex digits follows.
#[test]
fn a_non_string_font_is_not_a_font() {
    let theme = theme_of("theme odd:\n    font ui = 4\n    font named = \"sans\"\n");
    assert_eq!(fonts(&theme).tokens.len(), 1);
}

/// A theme with no fonts is empty rather than surprising, and a colour is not a font.
#[test]
fn a_theme_without_fonts_declares_none() {
    let theme = theme_of("theme bare:\n    color bg = 0x10121a\n");
    assert!(fonts(&theme).is_empty());
}
