//! The settings vocabulary, and the checker that holds a screen to it.
//!
//! `RUNTIME.md §2.1` is the store; this is the *list* — what a screen may name in a `preference` or
//! `toggle_preference` call. Both halves are tested here, because the value of the list is that a
//! name outside it fails where it is written: Ren'Py's `Preference("text speed")` is a string looked
//! up at run time, and a misspelled one is a control that silently does nothing.

use vela_diag::Diagnostic;
use vela_span::FileId;
use vela_syntax::{Item, ScreenDecl, parse};
use vela_ui::{
    ActionRegistry, SemanticActions, SettingDecl, SettingTy, WidgetRegistry, check_screen,
};

/// Every diagnostic a one-screen source produces.
fn diagnostics(source: &str) -> Vec<Diagnostic> {
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
    let screens: Vec<&ScreenDecl> = parsed
        .program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Screen(screen) => Some(screen),
            _ => None,
        })
        .collect();
    let registry = WidgetRegistry::builtin();
    let actions = ActionRegistry::builtin();
    screens
        .iter()
        .flat_map(|screen| {
            check_screen(
                &screen.body,
                &screen.params,
                &registry,
                &screens,
                &actions,
                &SemanticActions::builtin(),
            )
        })
        .collect()
}

/// The codes a source produces, in order.
fn codes(source: &str) -> Vec<String> {
    diagnostics(source)
        .iter()
        .map(|diagnostic| diagnostic.code.as_str().to_string())
        .collect()
}

/// The list is what the sample asks for, and not a name more.
///
/// Measured in `docs/roadmap/M12.2-game-interface.md`: twelve `Preference(…)` uses, nine distinct
/// settings — and the two that are missing here are missing on purpose. A volume is a *setting* whose
/// system is M12.3's audio, and declaring one now would be a slider that moves and changes nothing,
/// which is the drift the milestone's own **Still open** names.
#[test]
fn the_settings_are_the_ones_the_sample_asks_for() {
    let names = SettingDecl::names();
    assert_eq!(
        names,
        vec![
            "text_speed",
            "auto_forward",
            "auto_forward_time",
            "skip_unseen",
            "skip_after_choices",
            "transitions",
            "display_mode",
        ]
    );
    // Audio's, and M12.3's: the migration reports them by name rather than writing a control that
    // does nothing.
    for absent in ["music_volume", "sound_volume", "voice_volume", "mute_all"] {
        assert!(
            SettingDecl::named(absent).is_none(),
            "`{absent}` is M12.3's, and is declared with the audio that reads it"
        );
    }
}

/// A setting says what it holds and what it starts as, and the two agree.
#[test]
fn a_setting_declares_its_type_and_its_default() {
    let speed = SettingDecl::named("text_speed").expect("declared");
    assert_eq!(speed.ty, SettingTy::Number);
    assert!(
        speed.ty.takes(speed.default),
        "the default is a value it takes"
    );

    let skip = SettingDecl::named("skip_unseen").expect("declared");
    assert_eq!(skip.ty, SettingTy::Bool);
    assert!(skip.ty.takes(skip.default));

    let display = SettingDecl::named("display_mode").expect("declared");
    assert_eq!(display.ty, SettingTy::Choice(&["window", "fullscreen"]));
    assert!(display.ty.takes(display.default));
    assert!(!display.ty.takes("sideways"), "and not just anything");
}

/// Nothing reads a setting yet, and the declaration says so rather than implying it works.
///
/// The flag is the same honesty `ActionDecl::dispatched` carries: a vocabulary that runs ahead of its
/// systems is fine, and a reader being able to tell which are which is what makes it fine.
#[test]
fn a_setting_says_whether_anything_reads_it() {
    for setting in vela_ui::SETTINGS {
        assert!(
            !setting.read,
            "`{}` is read by nothing yet: the screens and the transport are item 4",
            setting.name
        );
        assert!(
            setting.summary().contains("not read yet"),
            "{}",
            setting.summary()
        );
        assert!(!setting.doc.is_empty(), "`{}` has no doc", setting.name);
    }
}

/// `E5020` — a name that is not a setting, with a suggestion when it is a typo.
#[test]
fn an_unknown_setting_is_reported_with_a_suggestion() {
    let found = diagnostics(
        "screen s():\n    button:\n        text \"Fast\"\n        action preference(\"text_speeed\", 30)\n",
    );
    assert_eq!(found[0].code.as_str(), "E5020");
    let help = found[0].help.clone().unwrap_or_default();
    assert!(help.contains("text_speed"), "{help}");

    // A name nowhere near anything declared gets the code and no guess.
    let stranger = diagnostics(
        "screen s():\n    button:\n        text \"Go\"\n        action toggle_preference(\"teleport\")\n",
    );
    assert_eq!(stranger[0].code.as_str(), "E5020");
    assert!(stranger[0].help.is_none(), "{:?}", stranger[0].help);
}

/// And a name that is *computed* is reported too: a setting is checked where it is written.
#[test]
fn a_setting_name_that_is_not_written_out_is_reported() {
    let found = diagnostics(
        "screen s(name):\n    button:\n        text \"Go\"\n        action preference(name, 1)\n",
    );
    assert_eq!(found[0].code.as_str(), "E5020");
    assert!(
        found[0].message.contains("written out"),
        "{}",
        found[0].message
    );
}

/// `E5021` — the right setting, a value it does not take.
#[test]
fn a_value_the_setting_does_not_take_is_reported() {
    assert_eq!(
        codes(
            "screen s():\n    button:\n        text \"Go\"\n        action preference(\"display_mode\", \"sideways\")\n"
        ),
        vec!["E5021"]
    );
    assert_eq!(
        codes(
            "screen s():\n    button:\n        text \"Go\"\n        action preference(\"skip_unseen\", \"yes\")\n"
        ),
        vec!["E5021"]
    );
    // A boolean has nothing to flip out of, so the toggle form says so under the same code.
    assert_eq!(
        codes(
            "screen s():\n    button:\n        text \"Go\"\n        action toggle_preference(\"text_speed\")\n"
        ),
        vec!["E5021"]
    );
}

/// A bare word is a *name*, and where a name points is not this check's question (`SCREENS.md §7.1`).
///
/// The boundary is deliberate and it is a test rather than a sentence: reading
/// `preference(display_mode, sideways)` as the word `sideways` would let the checker report a correct
/// screen the day a project declares that name, and a check that fails a right program is worse than
/// one that stays quiet. `E2001` is what asks where an undeclared name points.
#[test]
fn a_bare_word_as_a_value_is_left_to_the_screen_name_check() {
    assert!(
        codes(
            "screen s():\n    button:\n        text \"Go\"\n        action preference(display_mode, sideways)\n"
        )
        .is_empty()
    );
}

/// A well-written call reports nothing — including a value the checker cannot see whole, which is the
/// runtime's to answer (`SCREENS.md §7`).
#[test]
fn a_setting_a_screen_can_write_reports_nothing() {
    assert!(
        codes(
            "screen s(level):\n    button:\n        text \"Go\"\n        action preference(\"text_speed\", level)\n"
        )
        .is_empty()
    );
    assert!(
        codes(
            "screen s():\n    button:\n        text \"Window\"\n        action preference(\"display_mode\", window)\n    button:\n        text \"Fullscreen\"\n        action preference(\"display_mode\", \"fullscreen\")\n    button:\n        text \"Skip\"\n        action toggle_preference(skip_unseen)\n"
        )
        .is_empty()
    );
}

/// `setting("…")` is a *value*, and it answers the store the host injected — or the declaration, before
/// anybody has chosen (`RUNTIME.md §2.1`).
///
/// The arm that draws is the assertion: a question nothing answers draws the `else`, so naming the text
/// proves the value was read rather than answered `none` — which is the failure this file exists to catch.
#[test]
fn a_setting_answers_its_declaration_until_the_player_changes_it() {
    use vela_text::{Font, TextEngine};
    use vela_ui::{Args, Kind, Node, ScreenSet, ScreenState};

    let source = "screen s():\n    column:\n        if setting(\"skip_unseen\"):\n            text \"on\"\n        else:\n            text \"off\"\n        text setting(\"text_speed\")\n";
    let parsed = vela_syntax::parse(vela_span::FileId::from_raw(0), source);
    assert!(parsed.diagnostics.is_empty(), "the fixture must parse");
    let set = ScreenSet::from_items(&parsed.program.items);

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fonts/LiberationSans-Regular.ttf");
    let font = Font::from_bytes(std::fs::read(path).expect("read font"), 0).expect("load font");
    let mut text = TextEngine::new();
    text.add_font("sans", font);

    /// Every text a laid screen draws, in tree order.
    fn texts(node: &Node, out: &mut Vec<String>) {
        if let Kind::Text { text, .. } = &node.kind {
            out.push(text.clone());
        }
        for child in &node.children {
            texts(child, out);
        }
    }

    let mut drawn = Vec::new();
    let laid = set
        .lay(
            "s",
            &Args::new(),
            &ScreenState::new(),
            (1280, 720),
            &mut text,
            "sans",
        )
        .expect("the screen is declared");
    texts(&laid.node, &mut drawn);

    // Nobody has chosen anything: `skip_unseen` is declared `false`, and `text_speed` is `0`.
    assert_eq!(
        drawn,
        ["off", "0"],
        "a declaration is what the engine would do"
    );

    // A player who has: the same screen draws the same settings, from the store.
    let mut chosen = vela_world::Preferences::new();
    chosen.set("skip_unseen", vela_world::Value::Bool(true));
    chosen.set("text_speed", vela_world::Value::Int(45));
    let mut drawn = Vec::new();
    let laid = set
        .lay(
            "s",
            &Args::new().with_preferences(chosen),
            &ScreenState::new(),
            (1280, 720),
            &mut text,
            "sans",
        )
        .expect("the screen is declared");
    texts(&laid.node, &mut drawn);

    assert_eq!(
        drawn,
        ["on", "45"],
        "and the store is what the player chose"
    );
}

/// `E5020` — a `setting(...)` that names nothing, in either position a question can be written.
#[test]
fn a_question_that_names_no_setting_is_reported() {
    // In a value, where a screen reads one: `bar value = setting("…")`, `text setting("…")`.
    assert_eq!(
        codes("screen s():\n    column:\n        text setting(\"teleport\")\n"),
        vec!["E5020"]
    );
    // And in a condition, where a screen decides from one.
    assert_eq!(
        codes("screen s():\n    if setting(\"skip_unsean\"):\n        text \"on\"\n"),
        vec!["E5020"]
    );
    // A well-written question reports nothing, which is what says the two above are about the name.
    assert!(
        codes(
            "screen s():\n    column:\n        if setting(\"skip_unseen\"):\n            text \"on\"\n        bar value = setting(\"text_speed\")\n"
        )
        .is_empty()
    );
    // A question with a *computed* name cannot be checked either: the name is in scope, so nothing else
    // would report it (`SCREENS.md §7.1`).
    assert_eq!(
        codes("screen s(which):\n    if setting(which):\n        text \"on\"\n"),
        vec!["E5020"]
    );
}

/// A `use`d screen asks the host the same questions its caller does (`SCREENS.md §2.1`, §2.6).
///
/// What the host says travels with a call, the way an argument does: a screen that answered `setting(…)`
/// differently depending on who used it would be two screens. The failure this pins is the quiet one —
/// a scope built from nothing answers every setting with its *declaration*, so a panel `use`d inside a
/// settings screen would draw the engine's default while the screen around it drew what the player chose.
#[test]
fn a_used_screen_answers_the_store_its_caller_had() {
    use vela_text::{Font, TextEngine};
    use vela_ui::{Args, Kind, Node, ScreenSet, ScreenState};

    // The used screen is the one asking; the caller only places it.
    let source = "screen inner():\n    text setting(\"text_speed\")\n\nscreen outer():\n    column:\n        use inner\n";
    let parsed = vela_syntax::parse(vela_span::FileId::from_raw(0), source);
    assert!(parsed.diagnostics.is_empty(), "the fixture must parse");
    let set = ScreenSet::from_items(&parsed.program.items);

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fonts/LiberationSans-Regular.ttf");
    let font = Font::from_bytes(std::fs::read(path).expect("read font"), 0).expect("load font");
    let mut text = TextEngine::new();
    text.add_font("sans", font);

    /// Every text a laid screen draws, in tree order.
    fn texts(node: &Node, out: &mut Vec<String>) {
        if let Kind::Text { text, .. } = &node.kind {
            out.push(text.clone());
        }
        for child in &node.children {
            texts(child, out);
        }
    }

    let mut chosen = vela_world::Preferences::new();
    chosen.set("text_speed", vela_world::Value::Int(45));
    let laid = set
        .lay(
            "outer",
            &Args::new().with_preferences(chosen),
            &ScreenState::new(),
            (1280, 720),
            &mut text,
            "sans",
        )
        .expect("the screen is declared");

    let mut drawn = Vec::new();
    texts(&laid.node, &mut drawn);
    assert_eq!(drawn, ["45"], "the used screen saw the caller's store");
}
