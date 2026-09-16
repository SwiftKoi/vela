//! Command consumption: what the presenter does with the runtime's command stream.
//!
//! No GPU. A command becomes a draw list, and a draw list is plain data — so the mapping from
//! story to pixels is testable exactly, and a failure says *which rectangle* is wrong rather
//! than "the frame differs".

use vela_render::{DrawList, Presenter, Style};
use vela_text::{Font, TextEngine};
use vela_world::Command;

fn fresh() -> Presenter {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fonts/LiberationSans-Regular.ttf");
    let font = Font::from_bytes(std::fs::read(path).expect("read font"), 0).expect("load font");
    let mut text = TextEngine::new();
    text.add_font("sans", font);
    Presenter::new(text, "sans", (1280, 720))
}

fn say(speaker: Option<&str>, text: &str) -> Command {
    Command::Say {
        speaker: speaker.map(ToString::to_string),
        attributes: Vec::new(),
        text: text.to_string(),
        options: Vec::new(),
        transition: None,
    }
}

fn menu(choices: &[&str]) -> Command {
    Command::Menu {
        prompt: None,
        choices: choices
            .iter()
            .enumerate()
            .map(|(index, text)| vela_world::Choice {
                index,
                text: (*text).to_string(),
            })
            .collect(),
    }
}

fn stage(kind: vela_world::Stage, image: &str) -> Command {
    Command::Stage {
        kind,
        image: image.to_string(),
        attributes: Vec::new(),
        transforms: Vec::new(),
        transition: None,
    }
}

/// The background and the dialogue box, and nothing else, before anything is said.
#[test]
fn an_empty_story_draws_only_the_background() {
    let mut presenter = fresh();
    let mut draw = DrawList::new();
    presenter.build(&mut draw);
    assert_eq!(draw.rect_count(), 1, "expected just the background");
    assert!(draw.glyph_count() == 0);
}

/// A `Say` puts the text on screen — the whole point of the layer.
#[test]
fn a_say_draws_its_text() {
    let mut presenter = fresh();
    presenter.apply(&say(None, "Hello, world."));
    let mut draw = DrawList::new();
    presenter.build(&mut draw);

    assert!(draw.glyph_count() > 0, "a `Say` produced no glyphs at all");
    // Background, box.
    assert_eq!(
        draw.rect_count(),
        2,
        "expected a background and a dialogue box"
    );
}

#[test]
fn a_speaker_draws_above_the_line() {
    let mut with_speaker = fresh();
    with_speaker.apply(&say(Some("Eileen"), "The rain had stopped."));
    let mut draw = DrawList::new();
    with_speaker.build(&mut draw);
    let with = draw.glyph_count();

    let mut without = fresh();
    without.apply(&say(None, "The rain had stopped."));
    let mut draw = DrawList::new();
    without.build(&mut draw);

    assert!(with > draw.glyph_count(), "the speaker added no glyphs");
}

/// A later `Say` replaces an earlier one; a `Say` does not accumulate.
#[test]
fn the_newest_say_replaces_the_last() {
    let mut presenter = fresh();
    presenter.apply(&say(None, "First line."));
    presenter.apply(&say(None, "Second line."));
    let mut draw = DrawList::new();
    presenter.build(&mut draw);

    let single = {
        let mut other = fresh();
        other.apply(&say(None, "Second line."));
        let mut draw = DrawList::new();
        other.build(&mut draw);
        draw.glyph_count()
    };
    assert_eq!(draw.glyph_count(), single, "the first line was still drawn");
}

#[test]
fn scene_show_and_hide_track_the_stage() {
    let mut presenter = fresh();
    presenter.apply(&stage(vela_world::Stage::Scene, "bg.street"));
    assert_eq!(presenter.scene().images().len(), 1);

    presenter.apply(&stage(vela_world::Stage::Show, "eileen.angry"));
    assert_eq!(presenter.scene().images().len(), 2);

    presenter.apply(&stage(vela_world::Stage::Hide, "eileen.angry"));
    assert_eq!(presenter.scene().images().len(), 1);
    assert_eq!(presenter.scene().images()[0].image, "bg.street");

    // `scene` replaces rather than adds.
    presenter.apply(&stage(vela_world::Stage::Scene, "bg.forest"));
    assert_eq!(presenter.scene().images().len(), 1);
    assert_eq!(presenter.scene().images()[0].image, "bg.forest");
}

#[test]
fn staged_images_are_drawn() {
    let mut presenter = fresh();
    presenter.apply(&stage(vela_world::Stage::Scene, "bg.street"));
    let mut draw = DrawList::new();
    presenter.build(&mut draw);
    // Background, the placeholder, and the image's name.
    assert!(draw.rect_count() >= 2, "{:?}", draw.rect_count());
    assert!(draw.glyph_count() > 0, "the image name was not drawn");
}

/// Audio, pauses and transitions have no visual consequence, and are ignored rather than
/// refused — they are not this layer's business.
#[test]
fn commands_with_no_visual_effect_are_ignored() {
    let mut presenter = fresh();
    presenter.apply(&Command::Pause { seconds: Some(1.0) });
    presenter.apply(&Command::WaitClick);
    presenter.apply(&Command::Audio {
        kind: vela_world::Audio::Play,
        channel: "music".to_string(),
        source: Some("rain.ogg".to_string()),
        looping: true,
        fade: None,
    });
    let mut draw = DrawList::new();
    presenter.build(&mut draw);
    assert_eq!(draw.rect_count(), 1, "an audio command drew something");
}

/// A menu is state that waits: it stands until the story moves past it.
#[test]
fn a_menu_stands_until_the_next_command() {
    let mut presenter = fresh();
    assert!(presenter.menu().is_none());

    presenter.apply(&menu(&["Stay", "Leave"]));
    assert_eq!(presenter.menu().map(|menu| menu.len()), Some(2));
    assert_eq!(presenter.selection(), Some(0));

    // The answer to a menu is the next command arriving, so any other command clears it.
    presenter.apply(&say(None, "You stayed."));
    assert!(presenter.menu().is_none(), "the menu outlived its answer");
    assert_eq!(presenter.selection(), None);
}

/// The selection moves and wraps, and does nothing when there is no menu.
#[test]
fn the_selection_moves_and_wraps() {
    let mut presenter = fresh();
    assert!(!presenter.move_selection(1), "no menu to move in");

    presenter.apply(&menu(&["a", "b", "c"]));
    assert!(presenter.move_selection(1));
    assert_eq!(presenter.selection(), Some(1));
    assert!(presenter.move_selection(-1));
    assert_eq!(presenter.selection(), Some(0));
    assert!(presenter.move_selection(-1), "wraps to the end");
    assert_eq!(presenter.selection(), Some(2));
}

/// A menu draws its choices and a highlight; an empty menu draws no highlight.
#[test]
fn a_menu_draws_its_choices() {
    let mut presenter = fresh();
    presenter.apply(&say(None, "Where to?"));
    let mut without = DrawList::new();
    presenter.build(&mut without);

    presenter.apply(&menu(&["Stay inside", "Go outside"]));
    let mut with = DrawList::new();
    presenter.build(&mut with);

    assert!(
        with.glyph_count() > without.glyph_count(),
        "the choices added no text"
    );
    assert!(
        with.rect_count() > without.rect_count(),
        "the menu panel and its highlight added no rectangles"
    );
}

/// A menu with no choices is legal and draws nothing extra.
#[test]
fn an_empty_menu_draws_nothing() {
    let mut presenter = fresh();
    presenter.apply(&menu(&[]));
    let mut draw = DrawList::new();
    presenter.build(&mut draw);

    assert_eq!(draw.rect_count(), 1, "only the background");
    assert!(!presenter.move_selection(1));
}

/// `ARCHITECTURE.md §8`: a frame is a picture of state, so rebuilding without new commands
/// must produce the same frame.
#[test]
fn rebuilding_is_deterministic() {
    let mut presenter = fresh();
    presenter.apply(&say(Some("Eileen"), "The rain had stopped."));
    let mut first = DrawList::new();
    presenter.build(&mut first);
    let mut second = DrawList::new();
    presenter.build(&mut second);
    assert_eq!(first, second);
}

/// The placeholder tint is stable across processes — a hash that is *not* `DefaultHasher`,
/// which is seeded per process and would make a capture useless as a comparison.
#[test]
fn an_image_tint_is_stable() {
    assert_eq!(
        vela_render::tint_of("bg.street"),
        vela_render::tint_of("bg.street")
    );
    assert_ne!(
        vela_render::tint_of("bg.street"),
        vela_render::tint_of("bg.forest")
    );
}

#[test]
fn a_style_can_be_overridden() {
    let mut presenter = fresh();
    let style = Style {
        margin: 12.0,
        ..Style::default()
    };
    presenter.set_style(style);
    assert!((presenter.style().margin - 12.0).abs() < f32::EPSILON);
}

/// Long text wraps inside the box rather than running off the frame.
#[test]
fn long_dialogue_wraps_inside_the_box() {
    let mut presenter = fresh();
    let long = "The rain had stopped an hour ago, but the street still shone, and nobody had \
                come to close the shutters, so the light stayed where it was.";
    presenter.apply(&say(None, long));
    let mut draw = DrawList::new();
    presenter.build(&mut draw);

    let widest = draw
        .glyphs()
        .map(|glyph| glyph.x + glyph.width)
        .fold(0.0f32, f32::max);
    assert!(widest < 1280.0, "text ran off the frame: {widest}");
}

/// A text tag changes how the words are drawn, and is never drawn itself.
///
/// Bold is synthesized by drawing the glyphs twice a hair apart, because the project ships one
/// font face. Italic has no shear to lean with yet — that is a transform, and the draw list has
/// none until M13 — so it is drawn plain. The gap is asserted here so that it stays *stated*
/// rather than becoming a surprise in a screenshot.
#[test]
fn text_tags_are_read_rather_than_drawn() {
    let glyphs = |body: &str| {
        let mut presenter = fresh();
        presenter.apply(&say(None, body));
        let mut draw = DrawList::new();
        presenter.build(&mut draw);
        draw.glyph_count()
    };

    let plain = glyphs("Good Ending.");
    assert!(plain > 0, "the fixture should draw something");
    assert!(
        glyphs("{b}Good Ending.{/b}") > plain,
        "bold is a second pass over the same glyphs"
    );
    assert_eq!(
        glyphs("{i}Good Ending.{/i}"),
        plain,
        "italic has no face to lean, so it is drawn plain rather than not at all"
    );
    assert_eq!(
        glyphs("{b}Good{/b} Ending."),
        glyphs("Good Ending.") + glyphs("Good"),
        "the tag itself is read, never drawn"
    );
}
