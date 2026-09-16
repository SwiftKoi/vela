use vela_span::FileId;
use vela_syntax::parse;

use crate::{Env, check};

/// The type diagnostics for a single module.
fn codes(source: &str) -> Vec<String> {
    let file = FileId::from_raw(0);
    let parsed = parse(file, source);
    assert!(
        parsed.diagnostics.is_empty(),
        "fixture must parse cleanly: {:?}",
        parsed.diagnostics
    );

    let (env, mut diagnostics) = Env::build(&parsed.program);
    diagnostics.extend(check(&parsed.program, &env));
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str().to_string())
        .collect()
}

#[test]
fn a_well_typed_module_reports_nothing() {
    let source = "default score: int = 0\n\n\
                  label start:\n    var x: int = 1\n    var y = x + 2\n    \"score: [y]\"\n    return\n";
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}

#[test]
fn an_enum_with_no_variants_is_reported_e3001() {
    assert_eq!(codes("enum E:\n\nlabel a:\n    return\n"), vec!["E3001"]);
}

#[test]
fn an_optional_used_in_arithmetic_is_reported_e3002() {
    let source = "default maybe: int? = none\n\nlabel a:\n    var x = maybe + 1\n    return\n";
    assert_eq!(codes(source), vec!["E3002"]);
}

#[test]
fn an_optional_stored_as_its_payload_is_reported_e3002() {
    let source = "default maybe: int? = none\n\nlabel a:\n    var x: int = maybe\n    return\n";
    assert_eq!(codes(source), vec!["E3002"]);
}

#[test]
fn coalescing_unwraps_without_a_diagnostic() {
    let source =
        "default maybe: int? = none\n\nlabel a:\n    var x: int = maybe ?? 0\n    return\n";
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}

#[test]
fn a_declaration_whose_type_is_not_obvious_needs_one_e3003() {
    let source = "fn pick() -> int:\n    return 1\n\ndefault x = pick()\n\nlabel a:\n    return\n";
    assert_eq!(codes(source), vec!["E3003"]);
}

#[test]
fn an_annotated_call_is_fine() {
    let source =
        "fn pick() -> int:\n    return 1\n\ndefault x: int = pick()\n\nlabel a:\n    return\n";
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}

#[test]
fn branches_that_disagree_are_reported_e3004() {
    let source =
        "default c: bool = true\n\nlabel a:\n    var x = 1 if c else \"two\"\n    return\n";
    assert_eq!(codes(source), vec!["E3004"]);
}

#[test]
fn branches_that_agree_are_fine() {
    let source = "default c: bool = true\n\nlabel a:\n    var x = 1 if c else 2\n    return\n";
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}

#[test]
fn a_struct_in_an_interpolation_is_reported_e3005() {
    let source = "struct Route:\n    name: str\n\n\
                  label a:\n    var r = Route\n    var s = \"route [r]\"\n    return\n";
    assert_eq!(codes(source), vec!["E3005"]);
}

#[test]
fn a_scalar_in_an_interpolation_is_fine() {
    let source = "label a:\n    var n = 1\n    var s = \"n is [n]\"\n    return\n";
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}

#[test]
fn mixing_int_and_float_is_reported_e3006() {
    let source = "label a:\n    var x = 1 + 1.0\n    return\n";
    assert_eq!(codes(source), vec!["E3006"]);
}

#[test]
fn an_explicit_conversion_is_fine() {
    let source = "label a:\n    var x = float(1) + 1.0\n    return\n";
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}

#[test]
fn a_value_that_does_not_fit_its_annotation_is_reported_e3007() {
    let source = "label a:\n    var x: int = \"one\"\n    return\n";
    assert_eq!(codes(source), vec!["E3007"]);
}

#[test]
fn a_local_is_inferred_from_its_initialiser() {
    let source = "label a:\n    var x = 1\n    var y: int = x\n    return\n";
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}

#[test]
fn a_struct_field_has_its_declared_type() {
    let source = "struct Route:\n    name: str\n\n\
                  label a:\n    var r = Route\n    var n: int = r.name\n    return\n";
    assert_eq!(codes(source), vec!["E3007"]);
}

#[test]
fn an_enum_variant_has_the_enum_type() {
    let source =
        "enum Ending:\n    good\n\nlabel a:\n    var e: Ending = Ending.good\n    return\n";
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}

#[test]
fn a_for_binding_takes_the_element_type() {
    let source = "label a:\n    for n in [1, 2]:\n        var x: int = n\n    return\n";
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}

#[test]
fn a_match_binding_takes_the_payload_type() {
    let source = "enum E:\n    bad(reason: str)\n\n\
                  default x: E = E.bad\n\n\
                  label a:\n    match x:\n        when E.bad(reason):\n            var y: int = reason\n        else:\n            return\n    return\n";
    assert_eq!(codes(source), vec!["E3007"]);
}

#[test]
fn a_list_of_one_type_is_a_list() {
    let source = "label a:\n    var xs = [1, 2, 3]\n    var first: int = xs[0]\n    return\n";
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}

#[test]
fn a_condition_that_is_not_a_bool_is_reported() {
    let source = "label a:\n    if 1:\n        return\n    return\n";
    assert_eq!(codes(source), vec!["E3007"]);
}

#[test]
fn a_function_call_takes_its_declared_return_type() {
    let source = "fn pick() -> int:\n    return 1\n\n\
                  label a:\n    var x: int = pick()\n    return\n";
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}

#[test]
fn a_lambda_body_is_checked_with_its_parameters_in_scope() {
    let source = "label a:\n    var f = fn(v: int) -> v + 1\n    return\n";
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}

#[test]
fn a_parameter_of_the_wrong_type_in_a_lambda_is_reported() {
    let source = "label a:\n    var f = fn(v: int) -> v + 1.0\n    return\n";
    assert_eq!(codes(source), vec!["E3006"]);
}

#[test]
fn a_match_missing_a_variant_is_reported_e4001() {
    let source = "enum E:\n    good\n    bad\n\n\
                  default x: E = E.good\n\n\
                  label a:\n    match x:\n        when E.good:\n            return\n    return\n";
    assert_eq!(codes(source), vec!["E4001"]);
}

#[test]
fn a_match_covering_every_variant_is_fine() {
    let source = "enum E:\n    good\n    bad\n\n\
                  default x: E = E.good\n\n\
                  label a:\n    match x:\n        when E.good:\n            return\n        when E.bad:\n            return\n    return\n";
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}

#[test]
fn an_else_covers_whatever_is_left() {
    let source = "enum E:\n    good\n    bad\n\n\
                  default x: E = E.good\n\n\
                  label a:\n    match x:\n        when E.good:\n            return\n        else:\n            return\n    return\n";
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}

#[test]
fn an_arm_after_a_catch_all_is_reported_w4005() {
    let source = "enum E:\n    good\n    bad\n\n\
                  default x: E = E.good\n\n\
                  label a:\n    match x:\n        else:\n            return\n        when E.bad:\n            return\n    return\n";
    assert_eq!(codes(source), vec!["W4005"]);
}

#[test]
fn a_function_that_can_finish_without_returning_is_reported_e4002() {
    assert_eq!(
        codes("fn f(n: int) -> int:\n    var x = n\n"),
        vec!["E4002"]
    );
}

#[test]
fn a_function_that_returns_on_every_path_is_fine() {
    assert!(codes("fn f(n: int) -> int:\n    return n\n").is_empty());
}

/// The false positive this rule exists for: every variant returns, so every path returns — even
/// though no arm is an `else`.
#[test]
fn a_function_returning_from_every_arm_of_a_match_is_fine() {
    let source = "enum Ending:\n    good\n    bad\n\n\
                  fn rank(e: Ending) -> int:\n    match e:\n        when Ending.good:\n            \
                  return 1\n        when Ending.bad:\n            return 2\n";
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}

#[test]
fn a_guarded_arm_without_an_else_can_fall_through() {
    // The guard may be false, and nothing catches that case, so the function can finish
    // without returning.
    let source = "enum Ending:\n    good\n    bad\n\n\
                  fn rank(e: Ending, c: bool) -> int:\n    match e:\n        when Ending.good if c:\n            \
                  return 1\n        when Ending.bad:\n            return 2\n";
    assert_eq!(codes(source), vec!["E4002"]);
}

#[test]
fn a_guarded_arm_with_an_else_that_returns_is_fine() {
    let source = "enum Ending:\n    good\n    bad\n\n\
                  fn rank(e: Ending, c: bool) -> int:\n    match e:\n        when Ending.good if c:\n            \
                  return 1\n        else:\n            return 2\n";
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}

/// A missing arm is `E4001`'s to report. Repeating it as "this function does not return" would be
/// a second diagnostic for one mistake, and the fix is the same either way.
#[test]
fn a_match_missing_an_arm_is_reported_once() {
    let source = "enum Ending:\n    good\n    bad\n\n\
                  fn rank(e: Ending) -> int:\n    match e:\n        when Ending.good:\n            \
                  return 1\n";
    assert_eq!(codes(source), vec!["E4001"]);
}

#[test]
fn an_if_without_an_else_does_not_return_on_every_path() {
    let source = "fn f(c: bool) -> int:\n    if c:\n        return 1\n";
    assert_eq!(codes(source), vec!["E4002"]);
}

#[test]
fn an_if_with_both_branches_returning_does() {
    let source =
        "fn f(c: bool) -> int:\n    if c:\n        return 1\n    else:\n        return 2\n";
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}

#[test]
fn a_function_with_no_return_type_needs_no_return() {
    assert!(
        codes("fn f():\n    var x = 1\n").is_empty(),
        "{:?}",
        codes("fn f():\n    var x = 1\n")
    );
}

#[test]
fn a_constant_condition_is_reported_w4006() {
    let source = "label a:\n    if true:\n        return\n    return\n";
    assert_eq!(codes(source), vec!["W4006"]);
}

#[test]
fn a_real_condition_is_not_reported() {
    let source = "default c: bool = false\n\nlabel a:\n    if c:\n        return\n    return\n";
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}

#[test]
fn an_effect_call_type_checks() {
    let source = "effect rand.int(low: int, high: int) -> int\n\n\
                  label a:\n    var roll: int = rand.int(1, 6)\n    return\n";
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}

/// An effect's result is what the declaration promised, so it cannot be used as something
/// else without a conversion.
#[test]
fn an_effect_result_has_its_declared_type() {
    let source = "effect time.now() -> float\n\n\
                  label a:\n    var moment: int = time.now()\n    return\n";
    assert_eq!(codes(source), vec!["E3007"]);
}

#[test]
fn calling_an_effect_with_too_few_arguments_is_reported_e3008() {
    let source = "effect rand.int(low: int, high: int) -> int\n\n\
                  label a:\n    var roll = rand.int(1)\n    return\n";
    assert_eq!(codes(source), vec!["E3008"]);
}

#[test]
fn calling_an_effect_with_the_wrong_argument_type_is_reported_e3007() {
    let source = "effect rand.int(low: int, high: int) -> int\n\n\
                  label a:\n    var roll = rand.int(\"one\", 6)\n    return\n";
    assert_eq!(codes(source), vec!["E3007"]);
}

/// The same checking applies to a `fn`, which is what it was missing before: a declared
/// function's arguments were not looked at at all.
#[test]
fn calling_a_function_with_the_wrong_arity_is_reported_e3008() {
    let source = "fn clamp(n: int, low: int, high: int) -> int:\n    return n\n\n\
                  label a:\n    var x = clamp(1, 2)\n    return\n";
    assert_eq!(codes(source), vec!["E3008"]);
}

#[test]
fn calling_a_function_with_the_wrong_argument_type_is_reported_e3007() {
    let source = "fn double(n: int) -> int:\n    return n\n\n\
                  label a:\n    var x = double(\"two\")\n    return\n";
    assert_eq!(codes(source), vec!["E3007"]);
}

/// A parameter written without a type accepts any argument.
///
/// An absent type is `Unknown`, which fits everywhere and is never reported against
/// (`LANGUAGE.md §5.4`) — which is what lets a migrated screen's untyped parameters be called at all.
#[test]
fn an_untyped_parameter_accepts_any_argument() {
    let source = "fn takes(anything) -> int:\n    return 1\n\n\
                  label a:\n    var n: int = takes(\"a string\")\n    return\n";
    assert!(codes(source).is_empty(), "{:?}", codes(source));
}

/// The other half of that rule: a *typed* parameter still refuses the wrong argument, so the
/// relaxation above is an absence of a type rather than an absence of checking.
#[test]
fn a_typed_parameter_still_refuses_a_wrong_argument() {
    let source = "fn takes(n: int) -> int:\n    return n\n\n\
                  label a:\n    var x = takes(\"two\")\n    return\n";
    assert_eq!(codes(source), vec!["E3007"]);
}

/// A test's assertion is typed, so a test that could never pass says so before it runs.
///
/// This is the whole reason `expect` holds an expression rather than a string: an assertion written in
/// the language is an assertion the checker has already read.
#[test]
fn an_assertion_that_is_not_a_bool_is_reported() {
    let reported = codes(
        "default trust: int = 0\n\nlabel start:\n    return\n\n\
         test \"wrong\":\n    run\n    expect trust\n",
    );

    assert_eq!(reported, vec!["E3007"], "{reported:?}");
}

/// `choose` picks a menu option by its text, so its argument has to be text.
#[test]
fn a_choice_that_is_not_text_is_reported() {
    let reported = codes(
        "label start:\n    return\n\n\
         test \"wrong\":\n    choose 1\n",
    );

    assert_eq!(reported, vec!["E3007"], "{reported:?}");
}

/// And a well-typed test reports nothing, which is what says the assertion above is about a mistake
/// rather than about tests not being checked at all.
#[test]
fn a_well_typed_test_reports_nothing() {
    let reported = codes(
        "default trust: int = 0\n\nlabel start:\n    return\n\n\
         test \"right\":\n    run\n    expect trust == 0\n    choose \"Leave\"\n",
    );

    assert!(reported.is_empty(), "{reported:?}");
}
