//! Staged images against every module's declarations (`LANGUAGE.md §6.1`, `§7.5`).
//!
//! The check is *whole-program*, and the reason is the case the first test here is: the migrated
//! project's story stages `bg.lecturehall`, declared in another file. Written per module, the check
//! reported that — and `vela migrate`'s own output is not a project with a bug in it.

use crate::Session;

/// The codes a run produced, in order.
fn codes(session: &mut Session) -> Vec<String> {
    session
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code.as_str().to_string())
        .collect()
}

/// An image declared in one file resolves in another: the picture table is one table.
#[test]
fn an_image_resolves_across_modules() {
    let mut session = Session::new();
    session.set_file(
        "images.vela",
        "image bg.room = @\"art/room.png\"\nimage sylvie.green.normal = @\"art/s.png\"\n",
    );
    session.set_file(
        "story.vela",
        "label start:\n    scene bg.room\n    show sylvie.green.normal\n    hide sylvie\n    return\n",
    );
    session.set_assets(vec!["art/room.png".to_string(), "art/s.png".to_string()]);

    assert!(codes(&mut session).is_empty());
}

/// A name nothing declares is `E5019` — the runtime draws a *placeholder* for one, which is honest
/// rendering and a silent failure.
#[test]
fn a_name_nothing_declares_is_reported() {
    let mut session = Session::new();
    session.set_file(
        "main.vela",
        "image bg.room = @\"art/room.png\"\n\nlabel start:\n    scene bg.rom\n    return\n",
    );
    session.set_assets(vec!["art/room.png".to_string()]);

    let found = session.diagnostics();
    let reported: Vec<&str> = found
        .iter()
        .filter(|diagnostic| diagnostic.code.as_str() == "E5019")
        .map(|diagnostic| diagnostic.message.as_str())
        .collect();
    assert_eq!(reported.len(), 1, "{reported:?}");
    assert!(reported[0].contains("bg.rom"), "{reported:?}");
}

/// A bare tag resolves, because that is what a bare name means: `hide sylvie` names a sprite group.
#[test]
fn a_tag_of_a_declared_image_resolves() {
    let mut session = Session::new();
    session.set_file(
        "main.vela",
        "image sylvie.green.normal = @\"art/s.png\"\n\nlabel start:\n    hide sylvie\n    return\n",
    );
    session.set_assets(vec!["art/s.png".to_string()]);

    assert!(codes(&mut session).is_empty());
}

/// Every bad name is reported, and a stage deep inside a story's nesting is found.
#[test]
fn every_staged_name_is_checked_wherever_it_is_written() {
    let mut session = Session::new();
    session.set_file(
        "main.vela",
        "default seen: bool = false\n\nlabel start:\n    if seen:\n        show nowhere\n    menu:\n        \"Go\":\n            scene missing.too\n    return\n",
    );
    session.set_assets(Vec::new());

    assert_eq!(codes(&mut session), vec!["E5019", "E5019"]);
}
