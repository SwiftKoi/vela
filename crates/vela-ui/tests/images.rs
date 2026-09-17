//! Pictures in a screen: what a reference resolves to, and what it measures (`SCREENS.md §3`).
//!
//! Split from `instantiate.rs` by responsibility, following the same seam `styling.rs` did: that file
//! asks *what tree does a body build*, and this one asks *what does a leaf hold* — one question about
//! text and pictures, which is where a screen's content comes from.
//!
//! Neither file loads a picture. The table is the platform's (`vela_ui::images`), so a test supplies one
//! the way a runner does, with the sizes a real build would have measured.

use vela_span::FileId;
use vela_syntax::parse;
use vela_text::{Font, TextEngine};
use vela_ui::{Args, ImageTable, Kind, Picture, ScreenSet, Value};

/// The bundled face, so text measures to something real.
fn engine() -> TextEngine {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fonts/LiberationSans-Regular.ttf");
    let font = Font::from_bytes(std::fs::read(path).expect("read font"), 0).expect("load font");
    let mut text = TextEngine::new();
    text.add_font("sans", font);
    text
}

/// Compiles a one-file source, asserting it parses.
fn set(source: &str) -> ScreenSet {
    let parsed = parse(FileId::from_raw(0), source);
    assert!(
        parsed.diagnostics.is_empty(),
        "the fixture must parse: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| d.message.clone())
            .collect::<Vec<_>>()
    );
    ScreenSet::from_items(&parsed.program.items)
}

/// A picture with a texture and a size, as an upload would report it.
fn picture(texture: u32, width: u32, height: u32) -> Picture {
    Picture {
        texture,
        width,
        height,
    }
}

/// An `image` resolves its reference — a path written in the screen, or a value it was handed.
///
/// Two shapes and one rule for telling them apart (`SCREENS.md §3`): a path whose head is not in scope
/// is the picture's own name, and one the screen was *given* is a lookup. So a screen draws a fixed
/// picture and one chosen by data with the same syntax, and the size comes from the platform's table
/// either way — which is what keeps layout honest, since the screen cannot know how big a picture is.
#[test]
fn an_image_resolves_its_reference_and_size() {
    let source = "\
screen gallery(item):
    column:
        image bg.room
        image item.icon
";
    let images = ImageTable::from_entries([
        ("bg.room".to_string(), picture(1, 320, 180)),
        ("art.cave".to_string(), picture(2, 64, 48)),
    ]);
    let mut args = Args::new();
    args.set(
        "item",
        Value::Record(vec![(
            "icon".to_string(),
            Value::Str("art.cave".to_string()),
        )]),
    );

    let mut text = engine();
    let root = set(source)
        .with_images(images)
        .build("gallery", &args, &mut text, "sans", 1280.0)
        .expect("the screen is declared");
    let column = &root.children[0];

    let Kind::Image { name, size } = &column.children[0].kind else {
        panic!("expected a picture, got {:?}", column.children[0].kind);
    };
    assert_eq!(name, "bg.room", "a written path is the picture's name");
    assert_eq!(size.width, 320.0, "its size came from the table");

    let Kind::Image { name, size } = &column.children[1].kind else {
        panic!("expected a picture, got {:?}", column.children[1].kind);
    };
    assert_eq!(name, "art.cave", "a bound head is a value to look up");
    assert_eq!(size.height, 48.0);
}

/// A picture nothing supplies measures to nothing rather than to a guess.
///
/// The name is still resolved, because the reference is the screen's and the picture is the platform's:
/// a table that has not been filled yet is a build in progress, not a mistake, and the size it leaves
/// behind is the honest one.
#[test]
fn an_unknown_picture_measures_to_nothing() {
    let mut text = engine();
    let root = set("screen s:\n    image missing.thing\n")
        .build("s", &Args::new(), &mut text, "sans", 1280.0)
        .expect("the screen is declared");
    let Kind::Image { name, size } = &root.children[0].kind else {
        panic!("expected a picture");
    };
    assert_eq!(name, "missing.thing", "the name is still resolved");
    assert_eq!(size.width, 0.0, "a missing picture has no size to lay out");
}
