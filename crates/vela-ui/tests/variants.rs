//! Asking the host where this screen is running: `variant(...)` (`SCREENS.md §2.6`).
//!
//! Two sources, and the tests follow them apart: a *platform* name is declared by the bundle's
//! descriptor and reaches the set through `with_variants`, and the *frame* a screen is laid out in
//! decides `small` on its own. The conditions are the sample's own, because they are what the feature
//! is for — `renpy.variant("pc") or (renpy.variant("web") and not renpy.variant("mobile"))` is a real
//! line of `screens.rpy`, and a screen that draws the wrong arm of it draws the wrong menu.

use vela_span::FileId;
use vela_syntax::parse;
use vela_text::{Font, TextEngine};
use vela_ui::{Args, Kind, Node, ScreenSet, ScreenState, Value, Variant, Variants};

/// The bundled face, so text measures to something real.
fn engine() -> TextEngine {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fonts/LiberationSans-Regular.ttf");
    let font = Font::from_bytes(std::fs::read(path).expect("read font"), 0).expect("load font");
    let mut text = TextEngine::new();
    text.add_font("sans", font);
    text
}

/// A screen taking no arguments, whose arm is the assertion.
fn drawn(condition: &str, variants: Variants, size: (u32, u32)) -> String {
    let source = format!(
        "screen s:\n    column:\n        if {condition}:\n            text \"yes\"\n        else:\n            text \"no\"\n"
    );
    let parsed = parse(FileId::from_raw(0), &source);
    assert!(
        parsed.diagnostics.is_empty(),
        "the fixture must parse: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| d.message.clone())
            .collect::<Vec<_>>()
    );
    let mut text = engine();
    let laid = ScreenSet::from_items(&parsed.program.items)
        .with_variants(variants)
        .lay(
            "s",
            &Args::new(),
            &ScreenState::new(),
            size,
            &mut text,
            "sans",
        )
        .expect("the screen is declared");
    first_text(&laid.node).expect("both arms draw").to_string()
}

/// The text of the first `text` node.
fn first_text(node: &Node) -> Option<&str> {
    if let Kind::Text { text, .. } = &node.kind {
        return Some(text.as_str());
    }
    node.children.iter().find_map(first_text)
}

/// The set a descriptor's names declare.
fn declared(names: &[&str]) -> Variants {
    let (set, unknown) = Variants::declared(names);
    assert!(unknown.is_empty(), "these fixtures name known variants");
    set
}

/// The frame Vela's reference styles and the capture default are written against.
const REFERENCE: (u32, u32) = (1280, 720);

/// A phone held sideways, which is what the sample's `small` is about.
const PHONE: (u32, u32) = (640, 360);

/// A declared variant is what the descriptor said, and nothing else is.
#[test]
fn a_declared_variant_is_the_descriptors() {
    let pc = declared(&["pc"]);
    assert_eq!(drawn("variant(\"pc\")", pc, REFERENCE), "yes");
    assert_eq!(drawn("variant(\"web\")", pc, REFERENCE), "no");
    assert_eq!(drawn("variant(\"pc\")", Variants::new(), REFERENCE), "no");
}

/// The frame decides `small`, and it decides it per layout rather than once.
///
/// This is the half the bundle cannot answer: the same set, laid out in two frames, is small in one.
/// A window that is resized asks again, which is why the class is folded in at `lay` rather than
/// written into the set the runner installs.
#[test]
fn the_frame_decides_whether_it_is_small() {
    let none = Variants::new();
    assert_eq!(drawn("variant(\"small\")", none, REFERENCE), "no");
    assert_eq!(drawn("variant(\"small\")", none, PHONE), "yes");
    assert_eq!(drawn("not variant(\"small\")", none, REFERENCE), "yes");
    assert_eq!(drawn("not variant(\"small\")", none, PHONE), "no");
}

/// The two sources merge, and the sample's platform test means what it says.
///
/// Two lines of the sample, migrated unchanged: the side image is guarded by `not
/// renpy.variant("small")`, and the Help button by `pc or (web and not mobile)`.
#[test]
fn the_two_sources_merge() {
    let pc = declared(&["pc"]);
    let web = declared(&["web"]);
    let android = declared(&["web", "mobile"]);

    let help = "variant(\"pc\") or (variant(\"web\") and not variant(\"mobile\"))";
    assert_eq!(drawn(help, pc, REFERENCE), "yes");
    assert_eq!(drawn(help, web, REFERENCE), "yes");
    assert_eq!(drawn(help, android, REFERENCE), "no");
    assert_eq!(drawn(help, Variants::new(), REFERENCE), "no");

    // A phone bundle that is *also* small: the platform and the room are separate questions, and a
    // screen may ask both at once.
    assert_eq!(
        drawn("variant(\"mobile\") and variant(\"small\")", android, PHONE),
        "yes"
    );
    assert_eq!(
        drawn(
            "variant(\"mobile\") and variant(\"small\")",
            android,
            REFERENCE
        ),
        "no"
    );
}

/// A name the engine does not know is false, and never a crash.
///
/// Ren'Py answers `False` here too; the difference is that Vela reports the call (`E5018`), so an
/// author who wrote `tablet` — a variant Ren'Py has and Vela does not — is told rather than left with
/// an arm that never draws.
#[test]
fn a_name_the_engine_does_not_know_is_false() {
    let android = declared(&["web", "mobile"]);
    assert_eq!(drawn("variant(\"tablet\")", android, PHONE), "no");
    assert_eq!(drawn("variant(\"tv\")", android, PHONE), "no");
}

/// The vocabulary is closed, and a descriptor that asks for something outside it is refused.
///
/// The message names what is known, because the descriptor is read by a person debugging a build as
/// often as by the runtime, and "not a variant" without the list is a riddle.
#[test]
fn the_vocabulary_is_closed() {
    assert_eq!(Variant::named("pc"), Some(Variant::Pc));
    assert_eq!(Variant::named("small"), Some(Variant::Small));
    assert_eq!(Variant::named("tablet"), None);

    // A name this build cannot answer is handed back to the caller rather than refused: the caller is
    // what says so out loud, and nothing a screen can ask about depends on it.
    let (set, unknown) = Variants::declared(["pc", "tablet"]);
    assert_eq!(set.names(), vec!["pc"]);
    assert_eq!(unknown, vec!["tablet".to_string()]);

    let set = Variants::new().with(Variant::Web).with(Variant::Small);
    assert_eq!(set.names(), vec!["web", "small"]);
    assert!(set.has(Variant::Web) && set.has(Variant::Small) && !set.has(Variant::Pc));
}

/// A variant is a value as well as a condition, so a screen can draw or pass what it learned.
///
/// `Value::Bool` rather than a special type, because that is what the answer is — which is also what
/// lets `set_screen_variable` or a `use` argument carry it.
#[test]
fn a_variant_is_a_value() {
    let source = "screen s:\n    column:\n        text variant(\"pc\")\n";
    let parsed = parse(FileId::from_raw(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let mut text = engine();
    let laid = ScreenSet::from_items(&parsed.program.items)
        .with_variants(Variants::new().with(Variant::Pc))
        .lay(
            "s",
            &Args::new(),
            &ScreenState::new(),
            REFERENCE,
            &mut text,
            "sans",
        )
        .expect("the screen is declared");
    let drawn = first_text(&laid.node).expect("the text draws");
    assert_eq!(drawn, Value::Bool(true).as_text());
}
