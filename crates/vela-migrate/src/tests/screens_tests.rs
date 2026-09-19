//! What a `screens.rpy` becomes, one construct family at a time.
//!
//! The pass has two rules and this file is where each one is pinned. **A line is written only if it
//! still says something** — a widget whose picture was reported, a block whose every line was, a
//! style with nothing left goes rather than being emitted empty — and **what Vela has no counterpart
//! for is reported rather than approximated**, which for a screen means placement above all: a file
//! that kept `xpos 240` would look migrated and draw in the wrong place.
//!
//! The third rule is not a rule but a consequence, and most of these tests are about it: the file the
//! pass writes has to be one `vela check` accepts, which `crates/vela-cli/src/tests/migrate_tests.rs`
//! asserts end to end on a whole project. A lowering that reads well and does not compile is the
//! failure mode this pass had twice, and it is what these fixtures are shaped to catch.

use crate::Report;
use crate::gui::Names;

/// Lowers one `screens.rpy` against a theme that declares `styles` and holds `values`.
fn lower(rpy: &str, styles: &[&str]) -> (String, Vec<String>) {
    lower_with(rpy, styles, &[])
}

/// The same, with the theme's `gui.<name>` value map filled in.
fn lower_with(rpy: &str, styles: &[&str], values: &[(&str, &str)]) -> (String, Vec<String>) {
    let nodes = crate::read(rpy);
    let mut names = Names::default();
    for style in styles {
        names.styles.insert((*style).to_string());
    }
    for (name, value) in values {
        names
            .values
            .insert((*name).to_string(), (*value).to_string());
    }
    let mut report = Report::new();
    let text = crate::screens::lower("screens.rpy", &nodes, &names, &mut report)
        .map_or_else(String::new, |lowered| lowered.text);
    (text, entries(&report))
}

/// The report, as `what was reported: why` — a test asserts on whichever half it is about.
fn entries(report: &Report) -> Vec<String> {
    report
        .entries()
        .iter()
        .map(|entry| format!("{}: {}", entry.original, entry.reason))
        .collect()
}

/// Whether any entry mentions a word, which is how a test asks "was this reported?".
fn reported(entries: &[String], needle: &str) -> bool {
    entries.iter().any(|entry| entry.contains(needle))
}

/// The widget set: every Ren'Py name a sample uses, and what it becomes.
///
/// `fixed` is the one worth reading twice: it is a `stack` and not an `absolute`, because Vela's
/// `absolute` holds a single child and the registry says so (`E5004` when given two). The
/// coordinates a `fixed` places with are reported either way, which is why the container it becomes
/// is the half that matters.
#[test]
fn the_widgets_are_renamed() {
    let (text, _) = lower(
        "screen s():\n    hbox:\n        vbox:\n            null\n    fixed:\n        frame:\n            window:\n                label \"x\"\n",
        &[],
    );
    assert!(text.contains("row:"), "{text}");
    assert!(text.contains("column:"), "{text}");
    assert!(text.contains("spacer"), "{text}");
    assert!(text.contains("stack:"), "{text}");
    assert!(text.contains("box:"), "{text}");
    assert!(text.contains("text \"x\""), "{text}");
    assert!(!text.contains("fixed"), "{text}");
}

/// A widget with children keeps its colon, and one without does not get one.
///
/// This is the rule the pass got wrong in a way that pointed somewhere else entirely: `button`
/// without its colon made its children the next *siblings*, so a run of `E5005: no widget called
/// `action`` read like a rule about prop names rather than a missing character.
#[test]
fn a_block_header_keeps_its_colon() {
    let (text, _) = lower(
        "screen s():\n    vbox:\n        text \"x\"\n        null\n",
        &[],
    );
    assert!(text.contains("column:\n"), "{text}");
    assert!(text.contains("    text \"x\"\n"), "{text}");
}

/// `use` is a call, and a call may be a leaf or take a block.
///
/// Ren'Py's `use navigation` hands nothing over; `use game_menu("About"):` hands a block that the
/// used screen places wherever it writes `transclude` (`SCREENS.md §2.1`). One line, two shapes, and
/// the colon is what tells them apart.
#[test]
fn a_use_may_be_a_leaf_or_take_a_block() {
    let (text, _) = lower(
        "screen s():\n    use navigation\n    use game_menu(\"About\"):\n        text \"x\"\n",
        &[],
    );
    assert!(text.contains("use navigation\n"), "{text}");
    assert!(text.contains("use game_menu(\"About\"):\n"), "{text}");
}

/// `textbutton "Yes" action x` is a `button` holding a `text`, and the action keeps its name.
#[test]
fn a_textbutton_becomes_a_button_holding_a_text() {
    let (text, entries) = lower(
        "screen s():\n    textbutton \"Yes\" action Quit(confirm=False)\n",
        &[],
    );
    assert!(text.contains("button:\n"), "{text}");
    assert!(text.contains("    text \"Yes\"\n"), "{text}");
    // Vela's actions take positional arguments only (`E5013` counts them), so the word is dropped
    // and named rather than written.
    assert!(text.contains("    action quit()\n"), "{text}");
    assert!(reported(&entries, "confirm="), "{entries:?}");
}

/// An action Vela has no name for is reported, and the widget keeps the rest of what it said.
#[test]
fn an_action_with_no_vela_name_is_reported() {
    let (text, entries) = lower(
        "screen s():\n    textbutton \"Start\" action Start()\n",
        &[],
    );
    assert!(!text.contains("action Start"), "{text}");
    assert!(text.contains("text \"Start\""), "{text}");
    assert!(reported(&entries, "`Start` action"), "{entries:?}");
}

/// Placement is a skin's business; a matched pair of fractions is one of Vela's nine anchors.
///
/// `SCREENS.md §4.2` says a layout that needs arithmetic on pixel coordinates is the wrong layout, so
/// `xpos 240` is reported rather than translated — and `xalign 0.0` with `yalign 1.0` is a *point*
/// Vela has a name for, which is why the pair comes across and a lone axis does not.
#[test]
fn placement_is_reported_and_a_matched_pair_is_an_anchor() {
    // The pair as two lines under a widget, which is how a position is usually written...
    let (text, entries) = lower(
        "screen s():\n    null:\n        xalign 0.0\n        yalign 1.0\n    null:\n        xpos 240\n",
        &[],
    );
    assert!(text.contains("anchor bottom_left"), "{text}");
    assert!(!text.contains("240"), "{text}");
    assert!(reported(&entries, "xpos"), "{entries:?}");

    // ...and as two props on the widget's own line, which is how `add` writes one. Either way the
    // pair is one of the nine anchors and a lone axis is not.
    let (text, _) = lower(
        "screen s():\n    add \"art/room.png\" xalign 0.5 yalign 0.5\n",
        &[],
    );
    assert!(text.contains("anchor center"), "{text}");
}

/// A `gui.<name>` a screen reads is resolved to what the theme holds for it.
///
/// A style cannot reach a number, so `size = gui.title_text_size` has to become the number the theme
/// recorded — and a name the theme has no value for is reported rather than written, because the
/// migrated file would otherwise reference a token nothing declares.
#[test]
fn a_theme_value_is_resolved_and_an_unknown_one_is_reported() {
    let (text, entries) = lower_with(
        "style title_text:\n    size gui.title_text_size\n    color gui.accent_color\n",
        &[],
        &[
            ("gui.title_text_size", "50"),
            ("gui.accent_color", "theme.accent"),
        ],
    );
    assert!(text.contains("    size = 50\n"), "{text}");
    assert!(text.contains("    color = theme.accent\n"), "{text}");
    assert!(entries.is_empty(), "{entries:?}");

    // A style whose only value the theme cannot resolve is not written either: the entry names it,
    // and a style left out is better than a style that references nothing.
    let (text, entries) = lower("style title_text:\n    size gui.nothing_at_all\n", &[]);
    assert!(text.is_empty(), "{text}");
    assert!(reported(&entries, "gui.nothing_at_all"), "{entries:?}");
}

/// A style and a screen cannot share a name in Vela, and the style is the one that moves.
///
/// Ren'Py keeps the two namespaces apart and Vela keeps them in one, and the collision is real: the
/// GUI's styles are named after the groups a screen names itself with. The screen keeps the name
/// because that is what a `use` and an `open_screen` refer to.
#[test]
fn a_style_a_screen_took_the_name_of_is_renamed() {
    let (text, entries) = lower("screen main_menu():\n    text \"x\"\n", &["main_menu"]);
    assert!(text.contains("screen main_menu():"), "{text}");
    assert!(
        reported(&entries, "both a screen and a style"),
        "{entries:?}"
    );

    let (text, _) = lower(
        "style main_menu:\n    size 40\nscreen main_menu():\n    text \"x\"\n",
        &[],
    );
    assert!(text.contains("style main_menu_style:\n"), "{text}");
}

/// A picture is drawn when Vela can name it, and reported when the name belongs to something else.
///
/// `add bg.room` is an image Vela knows; `add SideImage()` is a call the host owns and `add
/// gui.main_menu_background` is a path in the skin, and either of those would be a name the migrated
/// project does not declare.
#[test]
fn a_picture_is_drawn_or_reported() {
    let (text, entries) = lower(
        "screen s():\n    add \"art/room.png\"\n    add bg.room\n    add SideImage()\n    add gui.main_menu_background\n",
        &[],
    );
    assert!(text.contains("image \"art/room.png\""), "{text}");
    assert!(text.contains("image bg.room"), "{text}");
    assert!(reported(&entries, "SideImage()"), "{entries:?}");
    assert!(
        reported(&entries, "gui.main_menu_background"),
        "{entries:?}"
    );
}

/// A string Vela cannot draw is reported, and the widget goes with it.
///
/// `[config.name!t]` is Vela's `[name]` with a namespace and a conversion flag Vela does not have,
/// and `{a=…}` is a text tag its presenter does not know (`LANGUAGE.md §5.5`). A `text` with nothing
/// to draw draws nothing, so the line is not written either.
#[test]
fn a_string_vela_cannot_draw_is_reported() {
    let (text, entries) = lower(
        "screen s():\n    text \"[config.name!t]\"\n    text \"{a=https://example.com}x{/a}\"\n    text \"plain\"\n",
        &[],
    );
    assert!(!text.contains("config.name"), "{text}");
    assert!(!text.contains("example.com"), "{text}");
    assert!(text.contains("text \"plain\""), "{text}");
    assert!(reported(&entries, "config.name!t"), "{entries:?}");
}

/// Ren'Py's translation id is a marker rather than words, so the words stay and the id is reported.
#[test]
fn a_translation_id_is_stripped_and_reported() {
    let (text, entries) = lower("screen s():\n    text \"{#auto_page}A\"\n", &[]);
    assert!(text.contains("text \"A\""), "{text}");
    assert!(!text.contains("auto_page"), "{text}");
    assert!(reported(&entries, "translation id"), "{entries:?}");
}

/// A second declaration of one screen is how Ren'Py gives a variant its own body, and Vela has one
/// screen per name — so the second is reported instead of being emitted into an `E2003`.
#[test]
fn a_second_declaration_of_a_screen_is_reported() {
    let (text, entries) = lower(
        "screen quick_menu():\n    text \"a\"\nscreen quick_menu():\n    text \"b\"\n",
        &[],
    );
    assert!(text.contains("text \"a\""), "{text}");
    assert!(!text.contains("text \"b\""), "{text}");
    assert!(reported(&entries, "declared twice"), "{entries:?}");
}

/// A loop over a producer nothing implements is reported, and the loop is written.
///
/// `range(6)` is Python's: nothing in a Vela screen produces a sequence yet (`SCREENS.md §2.4`'s
/// **Not yet.**), so the loop draws nothing until something does. Writing it is what keeps the
/// *structure* of a migrated screen readable; saying so is what keeps it honest.
#[test]
fn a_producer_that_does_not_exist_is_reported() {
    let (text, entries) = lower(
        "screen s():\n    for i in range(6):\n        text \"x\"\n",
        &[],
    );
    assert!(text.contains("for i in range(6):"), "{text}");
    assert!(reported(&entries, "range("), "{entries:?}");
}

/// A condition a call nothing answers guards cannot draw, so the arm is reported and left out.
///
/// The engine says the same thing as `W4013` and answers the condition false; writing the arm would
/// be a migrated screen that warns about itself.
#[test]
fn an_unanswerable_condition_is_reported_and_left_out() {
    let (text, entries) = lower(
        "screen s():\n    if GamepadExists():\n        text \"pad\"\n    else:\n        text \"keys\"\n",
        &[],
    );
    assert!(!text.contains("pad"), "{text}");
    assert!(text.contains("text \"keys\""), "{text}");
    assert!(reported(&entries, "GamepadExists"), "{entries:?}");
}

/// `renpy.variant("pc")` is Vela's `variant("pc")`, and the `variant` screen line is a system.
#[test]
fn a_variant_question_keeps_its_question() {
    let (text, entries) = lower(
        "screen s():\n    if renpy.variant(\"pc\"):\n        text \"x\"\n    variant \"touch\":\n        text \"y\"\n",
        &[],
    );
    assert!(text.contains("if variant(\"pc\"):"), "{text}");
    assert!(!text.contains("renpy."), "{text}");
    assert!(reported(&entries, "variant"), "{entries:?}");
}

/// A grid takes its column count and wraps, so Ren'Py's row count is reported; a `vpgrid` is the
/// same grid inside a viewport (`SCREENS.md §3.2`).
#[test]
fn a_grid_is_a_column_count_and_a_vpgrid_wraps_one() {
    let (text, entries) = lower(
        "screen s():\n    grid 3 2:\n        text \"x\"\n    vpgrid:\n        cols 2\n        yinitial 1.0\n        text \"y\"\n",
        &[],
    );
    assert!(text.contains("grid columns 3:"), "{text}");
    assert!(text.contains("viewport:"), "{text}");
    assert!(text.contains("grid columns 2:"), "{text}");
    assert!(text.contains("    initial 1.0\n"), "{text}");
    assert!(reported(&entries, "row count"), "{entries:?}");
}

/// A style Vela cannot paint is not declared, and a widget naming it is reported.
///
/// Ren'Py's styles are mostly a `properties` splat and placement — neither of which a Vela style
/// carries — so they arrive empty and are dropped, which is what makes a `style "namebox"` prop a
/// reference to a style the migrated module does not declare (`E5007`).
#[test]
fn a_style_that_paints_nothing_is_not_declared() {
    let (text, entries) = lower(
        "screen s():\n    text \"x\":\n        style \"namebox\"\nstyle namebox:\n    xpos 12\n",
        &[],
    );
    assert!(!text.contains("namebox:"), "{text}");
    assert!(!text.contains("style namebox"), "{text}");
    assert!(reported(&entries, "style namebox"), "{entries:?}");
}

/// A file with neither a screen nor a style is not this pass's to write.
#[test]
fn a_file_of_neither_is_left_alone() {
    assert!(
        crate::screens::lower(
            "other.rpy",
            &crate::read("label start:\n    return\n"),
            &Names::default(),
            &mut Report::new()
        )
        .is_none()
    );
}

/// A condition reads the theme the way a prop does, and a name no value holds is reported.
///
/// Both halves came from the sample. `gui.nvl_height` *is* in the theme — 115 — and the condition
/// kept the reference verbatim, so the number the theme recorded never reached the branch that asks
/// whether it is set. `gui.show_name` is declared in `options.rpy` rather than `gui.rpy`, so nothing
/// holds it, and the arm it guards was written into the migrated module with a name nothing
/// declares — where it *checked clean*, because a screen condition is not a place the checker
/// resolves names in.
#[test]
fn a_condition_reads_the_theme_and_reports_what_it_cannot() {
    let (text, _) = lower_with(
        "screen s():\n    if gui.nvl_height:\n        text \"nvl\"\n",
        &[],
        &[("gui.nvl_height", "115")],
    );
    assert!(text.contains("if 115:"), "{text}");

    let (text, entries) = lower(
        "screen s():\n    if gui.show_name:\n        text \"name\"\n",
        &[],
    );
    assert!(!text.contains("show_name"), "{text}");
    assert!(!text.contains("\"name\""), "{text}");
    assert!(reported(&entries, "gui.show_name"), "{entries:?}");
}

/// Ren'Py's own globals are *answered*, not kept: a name a migrated screen reads that Vela has not
/// got would read nothing, silently, and nothing would say so.
#[test]
fn renpys_own_globals_are_answered_and_reported() {
    let (text, entries) = lower(
        "screen s():\n    if main_menu:\n        text \"menu\"\n    if not _history_list:\n        text \"empty\"\n    if config.has_music:\n        text \"music\"\n",
        &[],
    );
    assert!(text.contains("if false:"), "{text}");
    assert!(text.contains("if not none:"), "{text}");
    assert!(reported(&entries, "main_menu"), "{entries:?}");
    assert!(reported(&entries, "_history_list"), "{entries:?}");
    assert!(reported(&entries, "config.has_music"), "{entries:?}");
}

/// A setting Ren'Py names by a label is translated into this language's word for it.
///
/// `Preference("skip", "toggle")` looks the label up at run time; Vela's name is checked where it is
/// written (`vela-ui::settings`), so the two are not the same string and this pass is what bridges
/// them: the label becomes the name, and `"toggle"` — a value word only the runtime could check —
/// becomes the action that flips a boolean.
#[test]
fn renpys_settings_are_translated_into_this_languages_names() {
    let (text, entries) = lower(
        "screen s():\n    button:\n        action Preference(\"display\", \"fullscreen\")\n    button:\n        action Preference(\"after choices\", \"toggle\")\n",
        &[],
    );

    assert!(
        text.contains("action preference(\"display_mode\", \"fullscreen\")"),
        "{text}"
    );
    assert!(
        text.contains("action toggle_preference(\"skip_after_choices\")"),
        "{text}"
    );
    assert!(entries.is_empty(), "{entries:?}");
}

/// A setting whose system is not built is reported, never written.
///
/// A volume control that moves a number nothing reads is the thing this pass exists not to produce,
/// and the entry names the milestone the audio lands in rather than leaving a dead control.
#[test]
fn a_setting_without_a_system_is_reported() {
    let (text, entries) = lower(
        "screen s():\n    button:\n        action Preference(\"all mute\", \"toggle\")\n",
        &[],
    );

    assert!(!text.contains("mute"), "{text}");
    assert!(reported(&entries, "all mute"), "{entries:?}");
    assert!(reported(&entries, "M12.3"), "{entries:?}");
}

/// A comment at column zero does not end a screen.
///
/// Ren'Py writes its own region markers that way — `the_question`'s `screens.rpy` has
/// `#begin language_picker` at column zero *inside* its `preferences` screen — and Ren'Py's lexer
/// drops comment-only lines before it looks at indentation, so the marker is trivia there and the
/// screen carries on. Read as structure it ended the screen: everything after it fell out of the
/// declaration, and a line outside every declaration is nobody's to report, so that screen lost its
/// language picker and five sliders with no entry saying so.
#[test]
fn a_comment_at_column_zero_does_not_end_a_screen() {
    let (text, _) = lower(
        "screen probe():\n    vbox:\n        label _(\"Before\")\n\n#begin region\n\n        label _(\"After\")\n\n#end region\n\n        label _(\"Last\")\n",
        &[],
    );

    assert!(text.contains("text \"Before\""), "{text}");
    assert!(text.contains("text \"After\""), "after the marker: {text}");
    assert!(
        text.contains("text \"Last\""),
        "and after the region: {text}"
    );
}

/// A bar a *setting* fills and writes is reported rather than written.
///
/// `value Preference("text speed")` is Ren'Py's **Value** object, not its action of the same name,
/// and lowering it as one wrote `preference("text speed")` — one argument to an action the registry
/// says takes two, which a migrated project failed `vela check` on. The bar goes with the prop: a
/// bar with no value says nothing, and the entry names the system that owns the line.
#[test]
fn a_bar_bound_to_a_setting_is_reported_rather_than_written() {
    let (text, entries) = lower(
        "screen s():\n    bar value Preference(\"text speed\")\n    bar value 5\n",
        &[],
    );

    assert!(!text.contains("Preference"), "{text}");
    assert!(
        text.contains("bar:"),
        "the bar that does say something: {text}"
    );
    assert!(reported(&entries, "value Preference(…)"), "{entries:?}");
}

/// A keyword inside a value is a keyword, not a name: `who is not None` kept the capital and named
/// nothing — which happened to read as false, and drew the right arm by accident.
#[test]
fn renpys_keywords_are_rewritten_inside_a_value() {
    let (text, _) = lower(
        "screen s(who):\n    if who is not None:\n        text \"x\"\n",
        &[],
    );
    assert!(text.contains("who is not none"), "{text}");
}
