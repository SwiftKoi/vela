//! The conversion between byte offsets and protocol positions.
//!
//! Two things are being pinned here, and the second is the one that would go unnoticed: that a
//! position is a position, and that a *column* is UTF-16 while a *span* is bytes. A test written only
//! in English would pass with the conversion missing entirely.

use vela_span::SourceMap;

use crate::position::{Position, offset, position, range};

/// A map holding one file, and the file's id.
fn sources(text: &str) -> (SourceMap, vela_span::FileId) {
    let mut map = SourceMap::new();
    let id = map.add("main.vela", text);
    (map, id)
}

#[test]
fn an_ascii_offset_is_its_column() {
    let (map, file) = sources("label a:\n    var x = 1\n");
    assert_eq!(
        position(&map, file, 0),
        Position {
            line: 0,
            character: 0
        }
    );
    // The first character of the second line: nine bytes in, which is also column 0.
    assert_eq!(
        position(&map, file, 9),
        Position {
            line: 1,
            character: 0
        }
    );
    assert_eq!(
        position(&map, file, 13),
        Position {
            line: 1,
            character: 4
        }
    );
}

/// One accented character is two bytes and *one* UTF-16 unit, so the column stops matching the byte
/// offset from there on.
#[test]
fn a_two_byte_character_counts_once() {
    let (map, file) = sources("café here\n");
    // `caf` is three bytes, `é` is bytes 3..5, and the space is byte 5.
    assert_eq!(
        position(&map, file, 3),
        Position {
            line: 0,
            character: 3
        }
    );
    assert_eq!(
        position(&map, file, 5),
        Position {
            line: 0,
            character: 4
        },
        "the byte offset is five, the UTF-16 column is four"
    );
}

/// An emoji outside the basic plane is four bytes and *two* UTF-16 units — a surrogate pair — which is
/// why the protocol counts units rather than characters: this is the case that would put a caret on
/// the wrong side of the character in a window that renders it.
#[test]
fn an_astral_character_counts_twice() {
    let (map, file) = sources("\"🎭\"\n");
    assert_eq!(
        position(&map, file, 5),
        Position {
            line: 0,
            character: 3
        },
        "quote, surrogate pair, quote: 5 bytes, 3 units"
    );
}

/// The conversion is a pair: every character boundary must survive the round trip, or a rename or a
/// goto would land one character off in a non-English file.
#[test]
fn every_character_boundary_round_trips() {
    let text = "label a:\n    \"héllo 🎭 wörld\"\n";
    let (map, file) = sources(text);

    for (byte, _) in text.char_indices() {
        let byte = byte as u32;
        let back = offset(&map, file, position(&map, file, byte));
        assert_eq!(back, Some(byte), "offset {byte} did not round trip");
    }
}

/// A column the protocol does not allow — inside a surrogate pair — rounds to the end of that
/// character rather than to a byte in the middle of it.
#[test]
fn a_column_inside_a_character_rounds_outwards() {
    let (map, file) = sources("\"🎭\"\n");
    // Unit 2 is the second half of the pair; the offset is the end of the character (byte 5).
    assert_eq!(
        offset(
            &map,
            file,
            Position {
                line: 0,
                character: 2
            }
        ),
        Some(5)
    );
}

#[test]
fn a_position_past_the_end_is_clamped_and_an_offset_past_the_end_is_none() {
    let (map, file) = sources("one\ntwo\n");

    // Clamped, not panicking: `vela-span` promises a diagnostic cannot crash the display.
    assert_eq!(
        position(&map, file, 9_999),
        Position {
            line: 2,
            character: 0
        }
    );
    assert_eq!(
        offset(
            &map,
            file,
            Position {
                line: 9,
                character: 0
            }
        ),
        None
    );
}

#[test]
fn a_span_becomes_a_range_with_both_ends_converted() {
    let text = "label a:\n    \"é\"\n    return\n";
    let (map, file) = sources(text);

    // The string literal, braces and all: from the opening quote to the closing one.
    let start = text.find('"').expect("a quote") as u32;
    let end = text.rfind('"').expect("a quote") as u32 + 1;
    let span = vela_span::Span::new(file, start, end);

    assert_eq!(
        range(&map, span),
        crate::position::Range {
            start: Position {
                line: 1,
                character: 4
            },
            end: Position {
                line: 1,
                character: 7
            },
        },
        "three code units: quote, é, quote"
    );
}
