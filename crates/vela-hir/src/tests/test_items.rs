//! `test` items: the references they declare, and where those references live.
//!
//! Apart from the story graph's own tests because the answer is different: a test names a label, and the
//! graph must *not* record an edge for it. A test does not run during the story, so an edge from nowhere
//! into a label would be a claim about what the story does.

use super::{codes, collect_src, resolve_src};

/// A test's `run from` is a reference, and resolution treats it as one.
///
/// It is resolved through the same function the story graph's transfers go through, so a test that
/// starts at a label nobody declares is `E5003` with the span of the path — the same diagnostic a
/// `jump` to that label gets, because it is the same mistake.
#[test]
fn a_test_that_starts_at_a_label_that_does_not_exist_is_reported() {
    let diagnostics =
        resolve_src("label start:\n    return\n\ntest \"from nowhere\":\n    run from nowhere\n");

    assert_eq!(codes(&diagnostics), vec!["E5003"]);
    assert!(
        diagnostics[0].message.contains("nowhere"),
        "{:?}",
        diagnostics[0]
    );
}

/// A qualified `run from` goes through the same rule a qualified `jump` does: the module has to be
/// imported before its labels can be named. The diagnostic is about the missing `use`, not about the
/// label — which is the difference between "you forgot the import" and "that label does not exist".
#[test]
fn a_test_cannot_name_a_module_it_did_not_import() {
    let diagnostics = resolve_src(
        "label start:\n    return\n\n\
         test \"from a chapter\":\n    run from chapters.forest.clearing\n",
    );

    assert_eq!(codes(&diagnostics), vec!["E2002"]);
}

/// And a test that starts where the story does declares no reference to resolve.
#[test]
fn a_bare_run_declares_no_reference() {
    let collected = collect_src(
        "main",
        "label start:\n    return\n\ntest \"the top\":\n    run\n",
    );

    assert!(
        collected.module.test_targets.is_empty(),
        "{:?}",
        collected.module.test_targets
    );
}

/// A test's reference is collected *outside* the story graph: a test does not run during the story, so
/// an edge from nowhere into a label would be a claim about what the story does.
#[test]
fn a_test_does_not_add_an_edge_to_the_story_graph() {
    let collected = collect_src(
        "main",
        "label start:\n    return\n\nlabel other:\n    return\n\n\
         test \"starts elsewhere\":\n    run from other\n",
    );

    let start = collected
        .module
        .story
        .find("start")
        .expect("`start` is a label");
    assert!(start.targets.is_empty(), "{:?}", start.targets);
    assert_eq!(collected.module.test_targets.len(), 1);
}
