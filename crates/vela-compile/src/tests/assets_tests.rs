//! Checking `@"path"` literals against a manifest (`LANGUAGE.md §7.5`).

use crate::Session;

/// The codes a run produced, in order.
fn codes(session: &mut Session) -> Vec<String> {
    session
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code.as_str().to_string())
        .collect()
}

/// A path the manifest has is not reported, and one it does not have is `E7001`.
#[test]
fn a_path_names_an_asset_or_it_is_reported() {
    let mut session = Session::new();
    session.set_file(
        "main.vela",
        "image hero = @\"art/hero.png\"\n\nlabel start:\n    play music @\"audio/theme.ogg\"\n    \"x\"\n    return\n",
    );
    session.set_assets(vec!["art/hero.png".to_string()]);

    let found = session.diagnostics();
    let reported: Vec<&str> = found
        .iter()
        .filter(|diagnostic| diagnostic.code.as_str() == "E7001")
        .map(|diagnostic| diagnostic.message.as_str())
        .collect();

    assert_eq!(reported.len(), 1, "{reported:?}");
    assert!(reported[0].contains("audio/theme.ogg"), "{reported:?}");
}

/// Every missing literal is reported, not just the first: a renamed directory is several
/// things to fix, and a list is one build where a sequence would be several.
#[test]
fn every_missing_path_is_reported() {
    let mut session = Session::new();
    session.set_file(
        "main.vela",
        "image one = @\"art/one.png\"\nimage two = @\"art/two.png\"\n\nlabel start:\n    \"x\"\n    return\n",
    );
    session.set_assets(vec![]);

    assert_eq!(codes(&mut session), vec!["E7001", "E7001"]);
}

/// A project with no `assets/` still has a manifest, and every literal in it is wrong.
///
/// This is the difference between `Some(vec![])` and nothing at all, and it is the whole
/// reason the two are distinct: "no assets" is a fact about the project that makes a literal
/// an error, while "no manifest" means nobody asked.
#[test]
fn an_empty_manifest_reports_everything() {
    let mut session = Session::new();
    session.set_file("main.vela", "label start:\n    \"x\"\n    return\n");
    session.set_assets(Vec::new());
    assert!(
        codes(&mut session).is_empty(),
        "no literals, nothing to report"
    );

    let mut with_a_path = Session::new();
    with_a_path.set_file(
        "main.vela",
        "image hero = @\"art/one.png\"\n\nlabel start:\n    \"x\"\n    return\n",
    );
    with_a_path.set_assets(Vec::new());
    assert_eq!(codes(&mut with_a_path), vec!["E7001"]);
}

/// Without a manifest the check does not run, so a single file compiled on its own is not
/// accused of referencing assets it has no way to know about.
#[test]
fn without_a_manifest_no_path_is_reported() {
    let mut session = Session::new();
    session.set_file(
        "main.vela",
        "image hero = @\"art/one.png\"\n\nlabel start:\n    \"x\"\n    return\n",
    );

    assert!(codes(&mut session).is_empty());
}

/// Setting the assets re-runs the analysis.
///
/// The set is read directly rather than through a tracked file, so it has no revision to
/// compare — which means nothing would invalidate the cached analysis if `set_assets` did not
/// do it itself. Without this test, "a diagnostic that never goes away" is the symptom.
#[test]
fn setting_the_assets_invalidates_the_analysis() {
    let mut session = Session::new();
    session.set_file(
        "main.vela",
        "image hero = @\"art/one.png\"\n\nlabel start:\n    \"x\"\n    return\n",
    );
    session.set_assets(Vec::new());
    assert_eq!(codes(&mut session), vec!["E7001"]);

    session.set_assets(vec!["art/one.png".to_string()]);
    assert!(
        codes(&mut session).is_empty(),
        "the manifest changed and the analysis did not re-run"
    );
}

/// A path literal mentioned twice is reported twice, because each one is a place to fix.
#[test]
fn a_repeated_path_is_reported_at_each_use() {
    let mut session = Session::new();
    session.set_file(
        "main.vela",
        "label start:\n    play music @\"audio/t.ogg\"\n    play music @\"audio/t.ogg\"\n    return\n",
    );
    session.set_assets(Vec::new());

    let found = session.diagnostics();
    let spans: Vec<u32> = found
        .iter()
        .filter(|diagnostic| diagnostic.code.as_str() == "E7001")
        .map(|diagnostic| diagnostic.primary.span.start())
        .collect();
    assert_eq!(spans.len(), 2, "{spans:?}");
    assert!(spans[0] < spans[1], "not in source order: {spans:?}");
}
