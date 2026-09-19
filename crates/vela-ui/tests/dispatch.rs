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

screen detail(title, page = 2):
    text title
    text page
";

/// A stack with `menu` open, and the sets to lay against.
fn opened() -> (ScreenSet, TextEngine, Stack) {
    let set = ScreenSet::from_items(&parse(FileId::from_raw(0), SCREENS).program.items);
    let mut text = engine();
    let mut stack = Stack::default();
    assert!(stack.open(&set, "menu", &[], (1280, 720), &mut text, "sans"));
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

    // `open_screen` lays the named screen out and pushes it. A name with no arguments is a call that
    // passes none: a screen whose parameters all have defaults opens from a name alone.
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

/// `replace_screen` shows a screen *in place of* the one asking, which is what a menu page is
/// (`SCREENS.md §7`).
///
/// The assertion is the stack's *depth*, because that is the whole difference between it and
/// `open_screen`: after a replacement one close empties the stack, and after an open it does not.
#[test]
fn a_menu_page_replaces_the_screen_it_was_opened_from() {
    let (set, mut text, mut stack) = opened();
    let size = (1280, 720);
    let replace = |screen: &str| action("replace_screen", vec![name(screen)]);

    assert_eq!(
        stack.dispatch(&replace("settings"), &set, size, &mut text, "sans"),
        Done::Replaced("settings".to_string())
    );
    assert_eq!(
        stack.top().map(|screen| screen.name.as_str()),
        Some("settings")
    );
    assert_eq!(stack.close().as_deref(), Some("settings"));
    assert!(
        stack.is_empty(),
        "the screen it replaced is not still under it"
    );

    // A name nothing declares is a missing screen and leaves the menu where it was: the page goes only
    // once its replacement is found.
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
        stack.dispatch(&replace("nosuchscreen"), &set, size, &mut text, "sans"),
        Done::Missing("nosuchscreen".to_string())
    );
    assert_eq!(
        stack.top().map(|screen| screen.name.as_str()),
        Some("settings"),
        "a typo does not dismiss the menu"
    );
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

/// Every word the top screen draws, in tree order — what a laid-out screen *says*.
fn words(stack: &Stack) -> Vec<String> {
    fn walk(node: &vela_ui::Node, out: &mut Vec<String>) {
        if let vela_ui::Kind::Text { text, .. } = &node.kind {
            out.push(text.clone());
        }
        for child in &node.children {
            walk(child, out);
        }
    }
    let mut out = Vec::new();
    if let Some(top) = stack.top() {
        walk(&top.laid.node, &mut out);
    }
    out
}

/// `open_screen(name, …)` passes the rest of the call to the screen it opens (`SCREENS.md §2.1`).
///
/// The values bind in the opened screen's **parameter order** — what a runtime can know, since an
/// action carries values and no argument names — and a parameter the call leaves out keeps its
/// declared default, which is the rule `use` binds by. What the screen *draws* is the assertion: a
/// binding that landed in a default instead would draw `2` where the call passed `7`.
#[test]
fn a_screen_opens_with_the_arguments_the_call_passed() {
    let (set, mut text, mut stack) = opened();
    let size = (1280, 720);

    assert_eq!(
        stack.dispatch(
            &action(
                "open_screen",
                vec![name("detail"), name("Chapter one"), Value::Num(7.0)]
            ),
            &set,
            size,
            &mut text,
            "sans"
        ),
        Done::Opened("detail".to_string())
    );
    assert_eq!(words(&stack), ["Chapter one", "7"]);

    // One argument is enough for a screen whose second parameter has a default — and the default is
    // evaluated by the same rule a `use` uses.
    assert_eq!(stack.close().as_deref(), Some("detail"));
    assert_eq!(
        stack.dispatch(
            &action("open_screen", vec![name("detail"), name("Chapter two")]),
            &set,
            size,
            &mut text,
            "sans"
        ),
        Done::Opened("detail".to_string())
    );
    assert_eq!(words(&stack), ["Chapter two", "2"]);

    // More values than the screen has parameters are dropped rather than refused: the checker is what
    // reports a call that cannot mean anything, and a screen mid-edit still opens.
    assert_eq!(stack.close().as_deref(), Some("detail"));
    assert_eq!(
        stack.dispatch(
            &action(
                "open_screen",
                vec![name("detail"), name("One"), Value::Num(3.0), name("extra")]
            ),
            &set,
            size,
            &mut text,
            "sans"
        ),
        Done::Opened("detail".to_string())
    );
    assert_eq!(words(&stack), ["One", "3"]);
}
