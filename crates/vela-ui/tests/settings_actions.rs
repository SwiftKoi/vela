//! What the two setting actions *do*: the write, the flip, and the mark a settings screen draws.
//!
//! `SCREENS.md §7.1` and `§5.1`, and the half of `vela-ui::settings` that is behaviour rather than
//! vocabulary — `tests/settings.rs` is the checker's half (a name that is not a setting, a value the
//! setting does not take). The split is the module's own: one list to hold a screen to, and one place that
//! acts on what a screen asked for.

/// `preference` writes the value the screen resolved, and `toggle_preference` flips.
///
/// The flip is the half worth pinning: a checkbox cannot read the setting (`SCREENS.md §7.1`), so a
/// toggle has to start from the *declaration's* default — `skip_unseen` is `false`, so the first press
/// makes it true, and `skip_after_choices` is `true`, so its first press makes it false.
#[test]
fn a_setting_is_written_by_the_action_a_screen_gives_it() {
    use vela_ui::actions::Action;
    use vela_ui::settings::write;
    use vela_ui::value::Value as ScreenValue;
    use vela_world::Value;

    let mut preferences = vela_world::Preferences::new();

    let speed = Action::new(
        "preference",
        vec![name("text_speed"), ScreenValue::Num(30.0)],
    );
    assert_eq!(
        write(&mut preferences, &speed),
        Some("text_speed".to_string())
    );
    assert_eq!(
        preferences.get("text_speed"),
        Some(&Value::Float(30.0)),
        "the number the screen resolved, not the words it wrote"
    );

    let skip = Action::new("toggle_preference", vec![name("skip_unseen")]);
    assert_eq!(
        write(&mut preferences, &skip),
        Some("skip_unseen".to_string())
    );
    assert_eq!(preferences.get("skip_unseen"), Some(&Value::Bool(true)));
    write(&mut preferences, &skip);
    assert_eq!(
        preferences.get("skip_unseen"),
        Some(&Value::Bool(false)),
        "and a second press flips it back"
    );

    let after = Action::new("toggle_preference", vec![name("skip_after_choices")]);
    write(&mut preferences, &after);
    assert_eq!(
        preferences.get("skip_after_choices"),
        Some(&Value::Bool(false)),
        "a default of `true` is what the first press flips *from*"
    );
}

/// An action that is not a setting, a name this build has not got, or a value the world cannot hold
/// writes nothing and answers nothing — a caller's to report.
#[test]
fn a_setting_that_cannot_be_written_is_refused() {
    use vela_ui::actions::Action;
    use vela_ui::settings::write;
    use vela_ui::value::Value as ScreenValue;

    let mut preferences = vela_world::Preferences::new();
    assert_eq!(write(&mut preferences, &Action::new("quit", vec![])), None);
    assert_eq!(
        write(
            &mut preferences,
            &Action::new("preference", vec![name("teleport"), ScreenValue::Num(1.0)])
        ),
        None,
        "a name the checker would have refused where it was written"
    );
    assert_eq!(
        write(
            &mut preferences,
            &Action::new(
                "preference",
                vec![
                    name("skip_unseen"),
                    ScreenValue::Action(Action::new("quit", vec![]))
                ]
            )
        ),
        None,
        "an action is a value a screen can hold and not one a setting can"
    );
    assert!(preferences.is_empty(), "nothing was written");
}

/// The mark a settings screen draws: which control writes what the store holds (`SCREENS.md §5.1`).
///
/// A declaration is a *typed* default rather than the text it was written as, and that is the half worth
/// pinning: `preference("text_speed", 0)` has to match the declaration `"0"` for the option the engine
/// would actually take to be the one that draws as chosen, and a string comparison would leave every
/// numeric setting's default unmarked.
#[test]
fn a_controls_selection_is_the_setting_it_writes() {
    use vela_ui::actions::Action;
    use vela_ui::settings::is_chosen;
    use vela_world::Value;

    let mut store = vela_world::Preferences::new();
    let window = Action::new("preference", vec![name("display_mode"), name("window")]);
    let fullscreen = Action::new("preference", vec![name("display_mode"), name("fullscreen")]);
    // The action's arguments are *screen* values, the store's are the world's (`SCREENS.md §7`): a
    // button writes `0` and `setting("text_speed")` answers a number, and the two are compared by value.
    let instant = Action::new(
        "preference",
        vec![name("text_speed"), vela_ui::value::Value::Num(0.0)],
    );
    let fast = Action::new(
        "preference",
        vec![name("text_speed"), vela_ui::value::Value::Num(60.0)],
    );
    let skip = Action::new("toggle_preference", vec![name("skip_unseen")]);

    // Nobody has chosen: the declarations decide, which is what makes a screen draw what the engine does.
    assert!(
        is_chosen(&store, &window),
        "`window` is the declared default"
    );
    assert!(!is_chosen(&store, &fullscreen));
    assert!(
        is_chosen(&store, &instant),
        "a number, not the text `\"0\"`"
    );
    assert!(!is_chosen(&store, &fast));
    assert!(
        !is_chosen(&store, &skip),
        "`skip_unseen` is declared `false`"
    );

    // A player who has: the store decides, and the mark moves without the screen changing a word.
    store.set("display_mode", Value::Str("fullscreen".to_string()));
    store.set("text_speed", Value::Int(60));
    store.set("skip_unseen", Value::Bool(true));
    assert!(!is_chosen(&store, &window));
    assert!(is_chosen(&store, &fullscreen));
    assert!(!is_chosen(&store, &instant));
    assert!(is_chosen(&store, &fast), "the store holds an int here");
    assert!(is_chosen(&store, &skip));
}

/// A setting's name, as a screen writes it.
fn name(text: &str) -> vela_ui::value::Value {
    vela_ui::value::Value::Str(text.to_string())
}
