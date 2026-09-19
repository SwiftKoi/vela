//! What an action does to the stack.
//!
//! `SCREENS.md §7` makes the action set a vocabulary; this is the half that acts on it. The dispatch
//! lives beside the stack because a screen asks three ways — the focused control, a `key` binding, and
//! a test that clicks (`TOOLING.md §5`) — and each caller only decides what to do with the *outcome*:
//! a window prints it, and a headless run reports what it cannot carry out.

use vela_span::FileId;
use vela_syntax::parse;
use vela_text::{Font, TextEngine};
use vela_ui::actions::Action;
use vela_ui::{Done, ScreenSet, Stack, Value};

/// A font, because laying a screen out needs one to size its text with.
fn engine() -> TextEngine {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fonts/LiberationSans-Regular.ttf");
    let font = Font::from_bytes(std::fs::read(path).expect("read font"), 0).expect("load font");
    let mut text = TextEngine::new();
    text.add_font("sans", font);
    text
}

/// Two screens, one of which opens the other; the second keeps a variable of its own.
const SCREENS: &str = "\
screen menu:
    column:
        button:
            text \"Settings\"
            action open_screen(settings)

screen settings:
    default tab = \"video\"
    button:
        text \"Video\"
        action set_screen_variable(tab, \"video\")
";

/// A stack with `menu` open, and the sets to lay against.
fn opened() -> (ScreenSet, TextEngine, Stack) {
    let set = ScreenSet::from_items(&parse(FileId::from_raw(0), SCREENS).program.items);
    let mut text = engine();
    let mut stack = Stack::default();
    assert!(stack.open(&set, "menu", (1280, 720), &mut text, "sans"));
    (set, text, stack)
}

/// An action a screen wrote: a name and its arguments.
fn action(name: &str, args: Vec<Value>) -> Action {
    Action::new(name, args)
}

/// A name, which is what every screen action takes.
fn name(text: &str) -> Value {
    Value::Str(text.to_string())
}

/// The four actions a stack owns, each ending where it says it did.
#[test]
fn the_stack_carries_out_what_a_screen_asks() {
    let (set, mut text, mut stack) = opened();
    let size = (1280, 720);

    // `open_screen` lays the named screen out and pushes it — no arguments, because a screen's
    // parameters have to have defaults for a name to be enough (`SCREENS.md §2.1`).
    assert_eq!(
        stack.dispatch(
            &action("open_screen", vec![name("settings")]),
            &set,
            size,
            &mut text,
            "sans"
        ),
        Done::Opened("settings".to_string())
    );
    assert_eq!(
        stack.top().map(|screen| screen.name.as_str()),
        Some("settings")
    );

    // A write is the *value* the screen resolved, not the words it wrote (`SCREENS.md §2.5`).
    assert_eq!(
        stack.dispatch(
            &action("set_screen_variable", vec![name("tab"), name("audio")]),
            &set,
            size,
            &mut text,
            "sans"
        ),
        Done::Set("tab".to_string())
    );

    // `hide` removes a screen by name wherever it sits, which `close_screen` cannot: closing "the top"
    // would dismiss whatever is above it.
    assert_eq!(
        stack.dispatch(
            &action("hide", vec![name("menu")]),
            &set,
            size,
            &mut text,
            "sans"
        ),
        Done::Hidden("menu".to_string())
    );
    assert_eq!(
        stack.top().map(|screen| screen.name.as_str()),
        Some("settings")
    );

    assert_eq!(
        stack.dispatch(
            &action("close_screen", vec![]),
            &set,
            size,
            &mut text,
            "sans"
        ),
        Done::Closed("settings".to_string())
    );
    assert!(stack.is_empty());
}

/// What the stack cannot do comes back named rather than done quietly — the caller has a VM and a
/// save system, or it has neither and says so.
#[test]
fn what_the_stack_does_not_own_comes_back_unhandled() {
    let (set, mut text, mut stack) = opened();
    let size = (1280, 720);

    // The vocabulary is bigger than the stack: `jump`, `quit` and a save are the VM's or the host's.
    for name in ["jump", "quit", "quick_save", "rollback", "play"] {
        assert_eq!(
            stack.dispatch(&action(name, vec![]), &set, size, &mut text, "sans"),
            Done::NotOurs,
            "`{name}` is not the stack's"
        );
    }

    // A screen nothing declares is *said*, not laid out as nothing.
    assert_eq!(
        stack.dispatch(
            &action("open_screen", vec![name("nosuchscreen")]),
            &set,
            size,
            &mut text,
            "sans"
        ),
        Done::Missing("nosuchscreen".to_string())
    );

    // And an action with nothing to act on is not an error: nothing is open to close, and an action
    // written without its argument is a screen the checker reports (`E5013`) rather than a stack rule.
    assert_eq!(stack.close().as_deref(), Some("menu"));
    assert_eq!(
        stack.dispatch(
            &action("close_screen", vec![]),
            &set,
            size,
            &mut text,
            "sans"
        ),
        Done::Nothing
    );
    assert_eq!(
        stack.dispatch(&action("hide", vec![]), &set, size, &mut text, "sans"),
        Done::Nothing
    );
}
