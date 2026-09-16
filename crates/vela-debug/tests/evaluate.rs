//! Checking an expression, which is what `evaluate` is for before it is for values.
//!
//! `TOOLING.md §6`: an invalid expression is a normal `Exxx` rather than an evaluator crash, and
//! the reason is that the *checker* is what answers. These tests are about that: the checker's
//! own diagnostics come back, a name in scope is known, and a name out of scope is not.

use vela_debug::evaluate;
use vela_span::FileId;

const STORY: &str = "\
default trust: int = 0

label start:
    var a: int = trust + 1
    \"a line\"
    return
";

/// An undefined name is the checker's own diagnostic, not an invented one.
#[test]
fn an_undefined_name_is_the_checkers_diagnostic() {
    let errors = evaluate(FileId::from_raw(0), "main.vela", STORY, "missing_name", &[])
        .expect_err("the name is not declared");

    assert!(
        errors.iter().any(|error| error.starts_with("E2001")),
        "expected the undefined-name code: {errors:?}"
    );
}

/// A valid expression answers its type, worked out from the world it mentions.
#[test]
fn a_valid_expression_answers_its_type() {
    let ty =
        evaluate(FileId::from_raw(0), "main.vela", STORY, "trust + 1", &[]).expect("it checks");
    assert_eq!(ty, "int");
}

/// A local in the paused frame is in scope, and its type comes from the frame declaration.
#[test]
fn a_frame_local_is_in_scope() {
    let ty = evaluate(
        FileId::from_raw(0),
        "main.vela",
        STORY,
        "a * 3",
        &[("a".to_string(), "int".to_string())],
    )
    .expect("the local is declared in the wrapper");
    assert_eq!(ty, "int");
}

/// And without it declared, the same expression is unknown — which is the honest answer for a
/// frame whose locals the caller did not hand over.
#[test]
fn a_local_that_is_not_in_scope_is_undefined() {
    let errors = evaluate(FileId::from_raw(0), "main.vela", STORY, "a * 3", &[])
        .expect_err("`a` is not declared in the wrapper");
    assert!(
        errors.iter().any(|error| error.starts_with("E2001")),
        "expected the undefined-name code: {errors:?}"
    );
}
