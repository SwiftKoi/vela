//! The screen pack through its public surface: it round-trips, and a version this build does not
//! know is refused.
//!
//! `SCREENS.md §13`. This is the artifact that lets a bundle run its interface with no parser in
//! the path, so the two things worth asserting are that the round trip preserves the screens and
//! that a foreign version is *refused* rather than read as if its unknown fields were absent.

use vela_ui::{PACK_VERSION, PackError, ScreenPack};

/// Compiles a source string into a pack, the way `vela build` does.
fn pack(source: &str) -> ScreenPack {
    let parsed = vela_syntax::parse(vela_span::FileId::from_raw(0), source);
    ScreenPack::compile("main", &parsed.program.items)
}

const SCREENS: &str = "\
theme dusk:
    color bg = 0x10121a
    font ui = \"sans\"

style body:
    color = theme.fg

screen dialogue(name: str?, line):
    text line style = body

screen pause:
    box stretch_x, stretch_y:
        text \"Paused\"
";

#[test]
fn a_pack_round_trips_into_the_same_screens() {
    let bytes = pack(SCREENS).to_bytes();
    let read = ScreenPack::from_bytes(&bytes).expect("a pack reads");

    let set = read.into_set();
    assert!(set.has("dialogue"), "the dialogue screen did not survive");
    assert!(set.has("pause"), "the pause screen did not survive");
    assert!(!set.has("settings"), "a screen was invented");
}

/// The theme's font tokens travel with the screens, because a bundle has no parser to read the theme
/// again and a style's `font` names one of them.
#[test]
fn a_pack_carries_the_themes_fonts() {
    let bytes = pack(SCREENS).to_bytes();
    let read = ScreenPack::from_bytes(&bytes).expect("a pack reads");
    assert_eq!(read.set.fonts.get("ui"), Some("sans"));
}

/// The container is a binary one: it starts with a magic number and is not source text.
#[test]
fn a_pack_is_a_binary_container() {
    let bytes = pack(SCREENS).to_bytes();
    assert!(bytes.starts_with(b"VELS"), "the magic is not at the front");
    assert!(
        !bytes.windows(6).any(|window| window == b"screen"),
        "the pack carries source text"
    );
}

/// A module that declares nothing for the interface compiles to a pack of nothing, so the build
/// can choose not to write one.
#[test]
fn a_module_with_no_screens_is_an_empty_pack() {
    assert!(pack("label start:\n    return\n").is_empty());
    assert!(!pack(SCREENS).is_empty());
}

/// The story is not in the pack: only the interface is.
#[test]
fn a_pack_does_not_carry_the_story() {
    let source = format!("{SCREENS}\nlabel start:\n    \"STORY-SENTINEL.\"\n    return\n");
    let bytes = pack(&source).to_bytes();
    assert!(
        !bytes.windows(14).any(|window| window == b"STORY-SENTINEL"),
        "the story leaked into the pack"
    );
}

#[test]
fn a_version_this_build_does_not_know_is_refused() {
    let mut bytes = pack(SCREENS).to_bytes();
    let newer = (PACK_VERSION + 1).to_le_bytes();
    bytes[4..6].copy_from_slice(&newer);

    let error = ScreenPack::from_bytes(&bytes).expect_err("a foreign version is refused");
    assert!(matches!(error, PackError::Version(_)), "{error:?}");
}

#[test]
fn a_pack_that_is_not_one_is_reported_rather_than_panicking() {
    let error = ScreenPack::from_bytes(b"not a pack at all").expect_err("garbage is refused");
    assert!(matches!(error, PackError::Malformed(_)), "{error:?}");
}
