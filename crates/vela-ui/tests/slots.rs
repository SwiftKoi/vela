//! What a save screen asks and is told: `slots(count)`, and the records it answers.
//!
//! `SCREENS.md §2.6` is the question and §7 the action it pairs with. The answer is the first *list* a
//! question gives, so what is asserted here is the shape a screen walks: one record per cell of the player's
//! page, in slot order, **empty cells included** — because a save screen draws the empty ones to be pressed,
//! and a screen cannot ask about a slot it was not offered.

use vela_span::FileId;
use vela_syntax::parse;
use vela_text::{Font, TextEngine};
use vela_ui::slots::{self, Slot};
use vela_ui::{Args, Node, ScreenSet, Value};

/// The bundled face, so laying a screen out measures something real.
fn engine() -> TextEngine {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fonts/LiberationSans-Regular.ttf");
    let font = Font::from_bytes(std::fs::read(path).expect("read font"), 0).expect("load font");
    let mut text = TextEngine::new();
    text.add_font("sans", font);
    text
}

/// A slot as a host would hand one over.
fn slot(page: u32, number: u32, name: &str, time: u64) -> Slot {
    Slot {
        page,
        number,
        name: name.to_string(),
        time,
        loadable: true,
    }
}

/// Every word a laid screen draws, in tree order.
fn texts(node: &Node, out: &mut Vec<String>) {
    if let vela_ui::Kind::Text { text, .. } = &node.kind
        && !text.is_empty()
    {
        out.push(text.clone());
    }
    for child in &node.children {
        texts(child, out);
    }
}

/// The page is `count` records in slot order, and the cells with nothing in them say so.
#[test]
fn a_page_is_one_record_per_cell() {
    let found = vec![slot(1, 1, "1-1", 1_700_000_000), slot(1, 3, "1-3", 5)];
    let answer = slots::page(&found, 1, 4);

    let numbers: Vec<&Value> = answer
        .iter()
        .filter_map(|cell| cell.field("number"))
        .collect();
    assert_eq!(
        numbers,
        vec![
            &Value::Num(1.0),
            &Value::Num(2.0),
            &Value::Num(3.0),
            &Value::Num(4.0),
        ],
        "one record per cell, in slot order"
    );

    // The occupied cells carry what the store knows.
    assert_eq!(
        answer[0].field("name"),
        Some(&Value::Str("1-1".to_string()))
    );
    assert_eq!(answer[0].field("empty"), Some(&Value::Bool(false)));
    assert_eq!(answer[0].field("time"), Some(&Value::Num(1_700_000_000.0)));
    assert_eq!(answer[0].field("loadable"), Some(&Value::Bool(true)));
    assert_eq!(
        answer[2].field("name"),
        Some(&Value::Str("1-3".to_string()))
    );

    // An empty cell is empty and *numbered*: the number is what a save button acts on.
    assert_eq!(answer[1].field("empty"), Some(&Value::Bool(true)));
    assert_eq!(answer[1].field("name"), Some(&Value::Str(String::new())));
    assert_eq!(answer[1].field("loadable"), Some(&Value::Bool(false)));

    // A page the host has nothing on is all empty cells, which is what a new game's save screen draws.
    let blank = slots::page(&found, 7, 2);
    assert_eq!(blank.len(), 2);
    assert!(
        blank
            .iter()
            .all(|cell| cell.field("empty") == Some(&Value::Bool(true)))
    );
}

/// A screen walks the answer: every cell of the page is drawn, and the empty ones are what a save presses.
#[test]
fn a_screen_draws_the_page_it_was_offered() {
    const SCREEN: &str = "screen file:\n    column:\n        for cell in slots(3):\n            if cell.empty:\n                text cell.number\n            else:\n                text cell.name\n";
    let parsed = parse(FileId::from_raw(0), SCREEN);
    assert!(parsed.diagnostics.is_empty(), "the fixture must parse");
    let set = ScreenSet::from_items(&parsed.program.items);

    // A page with one slot in the middle of it: the other two cells have to be drawn as empty *cells*, with
    // their numbers, or a save screen would have nothing to press for them.
    let args = Args::new().with_slots(vec![slot(1, 2, "1-2", 0)]);
    let mut text = engine();
    let laid = set
        .lay(
            "file",
            &args,
            &vela_ui::ScreenState::new(),
            (640, 480),
            &mut text,
            "sans",
        )
        .expect("the screen is declared");

    let mut drawn = Vec::new();
    texts(&laid.node, &mut drawn);
    assert_eq!(drawn, vec!["1", "1-2", "3"]);
}

/// A page that has rolled over answers the page the player is on, not the one the slots are on.
#[test]
fn the_page_comes_from_the_players_setting() {
    const SCREEN: &str = "screen file:\n    column:\n        for cell in slots(2):\n            if cell.empty:\n                text cell.number\n            else:\n                text cell.name\n";
    let parsed = parse(FileId::from_raw(0), SCREEN);
    let set = ScreenSet::from_items(&parsed.program.items);

    let slots = vec![slot(1, 1, "1-1", 0), slot(2, 1, "2-1", 0)];
    let mut preferences = vela_world::Preferences::new();
    let mut text = engine();

    // Nobody has paged: the first page, and only its own slots.
    let first = set
        .lay(
            "file",
            &Args::new()
                .with_slots(slots.clone())
                .with_preferences(preferences.clone()),
            &vela_ui::ScreenState::new(),
            (640, 480),
            &mut text,
            "sans",
        )
        .expect("declared");
    let mut drawn = Vec::new();
    texts(&first.node, &mut drawn);
    assert_eq!(
        drawn,
        vec!["1-1", "2"],
        "page one: its own slot, then the number of the empty cell beside it"
    );

    // Paged to the second: the same screen draws the slot that lives there.
    preferences.set("file_page", vela_world::Value::Int(2));
    let second = set
        .lay(
            "file",
            &Args::new().with_slots(slots).with_preferences(preferences),
            &vela_ui::ScreenState::new(),
            (640, 480),
            &mut text,
            "sans",
        )
        .expect("declared");
    let mut drawn = Vec::new();
    texts(&second.node, &mut drawn);
    assert_eq!(
        drawn,
        vec!["2-1", "2"],
        "page two: the same cell, the slot that lives there"
    );
}
