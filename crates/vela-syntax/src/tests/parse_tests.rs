use vela_span::FileId;

use crate::parse::{self, ParseResult};
use crate::tree::{BinOp, Expr, Item, Stmt, StrPart};

pub(super) fn parse_src(src: &str) -> ParseResult {
    parse::parse(FileId::from_raw(0), src)
}

fn codes(src: &str) -> Vec<String> {
    parse_src(src)
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str().to_string())
        .collect()
}

/// The label a single-label file declares.
fn only_label(src: &str) -> Vec<Stmt> {
    let result = parse_src(src);
    assert!(
        result.diagnostics.is_empty(),
        "expected a clean parse, got {:?}",
        result.diagnostics
    );
    match result.program.items.into_iter().next() {
        Some(Item::Label(label)) => label.body,
        other => panic!("expected a label, got {other:?}"),
    }
}

#[test]
fn the_hello_world_fixture_parses_cleanly() {
    let body = only_label("label start:\n    \"Hello, world.\"\n    return\n");
    assert_eq!(body.len(), 2);
    assert!(matches!(body[0], Stmt::Say(_)));
    assert!(matches!(body[1], Stmt::Return(_)));
}

#[test]
fn the_character_fixture_parses_cleanly() {
    let src = "character eileen:\n    name = \"Eileen\"\n    color = 0xf2a2b0\n\nlabel start:\n    eileen \"Hi.\"\n";
    let result = parse_src(src);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(result.program.items.len(), 2);
}

#[test]
fn a_say_statement_carries_speaker_attributes_and_transition() {
    let body = only_label("label a:\n    eileen sad \"Hi.\" with dissolve\n");
    let Stmt::Say(say) = &body[0] else {
        panic!("expected a say statement, got {:?}", body[0]);
    };
    assert_eq!(say.speaker.as_deref(), Some("eileen"));
    assert_eq!(say.attributes, vec!["sad"]);
    assert_eq!(say.transition.as_deref(), Some("dissolve"));
}

#[test]
fn a_bare_string_is_narration() {
    let body = only_label("label a:\n    \"Just rain.\"\n");
    let Stmt::Say(say) = &body[0] else {
        panic!("expected a say statement");
    };
    assert!(say.speaker.is_none());
}

#[test]
fn a_menu_parses_its_prompt_and_choices() {
    let src = "label a:\n    menu \"Pick\":\n        \"A\" if trust > 1:\n            jump b\n        \"B\":\n            jump c\n";
    let body = only_label(src);
    let Stmt::Menu(menu) = &body[0] else {
        panic!("expected a menu, got {:?}", body[0]);
    };
    assert!(menu.prompt.is_some());
    assert_eq!(menu.choices.len(), 2);
    assert!(menu.choices[0].condition.is_some());
    assert!(menu.choices[1].condition.is_none());
}

#[test]
fn multiplication_binds_tighter_than_addition() {
    let body = only_label("label a:\n    var x = 1 + 2 * 3\n");
    let Stmt::Var(var) = &body[0] else {
        panic!("expected a var statement");
    };
    let Expr::Binary { op, rhs, .. } = &var.value else {
        panic!("expected a binary expression, got {:?}", var.value);
    };
    assert_eq!(*op, BinOp::Add);
    assert!(matches!(**rhs, Expr::Binary { op: BinOp::Mul, .. }));
}

#[test]
fn parentheses_override_precedence() {
    let body = only_label("label a:\n    var x = (1 + 2) * 3\n");
    let Stmt::Var(var) = &body[0] else {
        panic!("expected a var statement");
    };
    let Expr::Binary { op, lhs, .. } = &var.value else {
        panic!("expected a binary expression");
    };
    assert_eq!(*op, BinOp::Mul);
    assert!(matches!(**lhs, Expr::Paren { .. }));
}

#[test]
fn postfix_chains_parse_left_to_right() {
    let body = only_label("label a:\n    var x = f(1).g[2]\n");
    let Stmt::Var(var) = &body[0] else {
        panic!("expected a var statement");
    };
    // `f(1).g[2]` is an index into a field of a call.
    let Expr::Index { base, .. } = &var.value else {
        panic!("expected an index expression, got {:?}", var.value);
    };
    let Expr::Field { base, name, .. } = &**base else {
        panic!("expected a field access");
    };
    assert_eq!(name, "g");
    assert!(matches!(**base, Expr::Call { .. }));
}

#[test]
fn list_and_map_literals_parse() {
    let body = only_label("label a:\n    var xs = [1, 2, 3]\n    var m = {\"k\": 1}\n");
    let Stmt::Var(list) = &body[0] else {
        panic!("expected a var statement");
    };
    assert!(matches!(&list.value, Expr::List { items, .. } if items.len() == 3));

    let Stmt::Var(map) = &body[1] else {
        panic!("expected a var statement");
    };
    assert!(matches!(&map.value, Expr::Map { entries, .. } if entries.len() == 1));
}

#[test]
fn a_string_splits_into_literal_and_interpolated_parts() {
    let body = only_label("label a:\n    var s = \"a {score} b\"\n");
    let Stmt::Var(var) = &body[0] else {
        panic!("expected a var statement");
    };
    let Expr::Str { parts, .. } = &var.value else {
        panic!("expected a string, got {:?}", var.value);
    };
    assert_eq!(parts.len(), 3);
    assert!(matches!(&parts[0], StrPart::Literal { text, .. } if text == "a "));
    assert!(matches!(&parts[1], StrPart::Interpolation { .. }));
    assert!(matches!(&parts[2], StrPart::Literal { text, .. } if text == " b"));
}

#[test]
fn an_interpolated_expression_keeps_absolute_spans() {
    let src = "label a:\n    var s = \"a {score} b\"\n";
    let body = only_label(src);
    let Stmt::Var(var) = &body[0] else {
        panic!("expected a var statement");
    };
    let Expr::Str { parts, .. } = &var.value else {
        panic!("expected a string");
    };
    let StrPart::Interpolation { expr, .. } = &parts[1] else {
        panic!("expected an interpolation");
    };

    // The inner expression must point at `score` in the real file, not in the
    // extracted fragment, or every diagnostic inside `{...}` would point at nonsense.
    let span = expr.span();
    let text = &src[span.start() as usize..span.end() as usize];
    assert_eq!(text, "score");
}

#[test]
fn an_escaped_brace_is_literal_text() {
    let body = only_label("label a:\n    var s = \"a \\{ b\"\n");
    let Stmt::Var(var) = &body[0] else {
        panic!("expected a var statement");
    };
    let Expr::Str { parts, .. } = &var.value else {
        panic!("expected a string");
    };
    assert_eq!(parts.len(), 1);
    assert!(matches!(&parts[0], StrPart::Literal { text, .. } if text == "a { b"));
}

#[test]
fn a_match_parses_patterns_guards_and_else() {
    let src = "label a:\n    match warmth:\n        when Ending.good:\n            return\n        when Ending.bad(reason) if reason is not none:\n            return\n        else:\n            return\n";
    let body = only_label(src);
    let Stmt::Match(stmt) = &body[0] else {
        panic!("expected a match, got {:?}", body[0]);
    };
    assert_eq!(stmt.arms.len(), 3);
    assert!(stmt.arms[0].pattern.is_some());
    assert!(stmt.arms[1].guard.is_some());
    assert!(stmt.arms[2].pattern.is_none(), "the last arm is `else`");
}

#[test]
fn if_elif_else_parses_all_branches() {
    let src = "label a:\n    if x:\n        \"one\"\n    elif y:\n        \"two\"\n    else:\n        \"three\"\n";
    let body = only_label(src);
    let Stmt::If(stmt) = &body[0] else {
        panic!("expected an if, got {:?}", body[0]);
    };
    assert_eq!(stmt.elifs.len(), 1);
    assert!(stmt.else_body.is_some());
}

#[test]
fn a_standalone_with_statement_parses() {
    let body = only_label("label a:\n    with dissolve\n");
    assert!(matches!(&body[0], Stmt::With(w) if w.transition == "dissolve"));
}

#[test]
fn stage_statements_parse_their_clauses() {
    // An image name is a dotted path; a bare word after it is an image *attribute*,
    // which selects a variant. `scene bg.forest` names the image; `show eileen happy`
    // names `eileen` and selects `happy`.
    let body = only_label(
        "label a:\n    scene bg.forest with fade\n    show eileen happy at left, up\n    hide eileen\n",
    );
    let Stmt::Stage(scene) = &body[0] else {
        panic!("expected a stage statement");
    };
    assert_eq!(scene.image, vec!["bg", "forest"]);
    assert!(scene.transition.is_some());

    let Stmt::Stage(show) = &body[1] else {
        panic!("expected a stage statement");
    };
    assert_eq!(show.attributes, vec!["happy"]);
    assert_eq!(show.transforms, vec!["left", "up"]);

    let Stmt::Stage(hide) = &body[2] else {
        panic!("expected a stage statement");
    };
    assert!(hide.attributes.is_empty() && hide.transforms.is_empty());
}

#[test]
fn declarations_parse() {
    let src = "\
use chapters.forest as forest

struct Route:
    name: str
    unlocked: bool = false

enum Ending:
    good
    bad(reason: str)

fn pick(n: int) -> str:
    return \"x\"

theme dusk:
    color = 0x10121a
    space = 4
";
    let result = parse_src(src);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(result.program.items.len(), 5);
}

#[test]
fn a_screen_body_is_consumed_so_the_next_item_still_parses() {
    // Screen bodies are a widget tree, parsed at M7. Until then the body is skipped,
    // but the *next* item must still parse — otherwise every file with a screen in it
    // would break.
    let src = "screen s(a: int):\n    box:\n        text \"hi\"\n\nlabel after:\n    return\n";
    let result = parse_src(src);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(result.program.items.len(), 2);
    assert!(matches!(result.program.items[1], Item::Label(_)));
}

// --- errors and recovery -------------------------------------------------------

#[test]
fn a_missing_expression_is_one_diagnostic_and_the_rest_still_parses() {
    let src = "label a:\n    \"ok\"\n    var x = \n    \"also ok\"\n";
    let result = parse_src(src);
    assert_eq!(codes(src), vec!["E1003"]);

    let Item::Label(label) = &result.program.items[0] else {
        panic!("expected a label");
    };
    assert_eq!(label.body.len(), 3, "{:?}", label.body);
}

#[test]
fn a_block_that_is_not_indented_is_reported() {
    // Two diagnostics: the label has no block, and the stray `return` that follows is
    // not a top-level declaration either.
    let found = codes("label a:\nreturn\n");
    assert!(found.contains(&"E1002".to_string()), "{found:?}");
}

#[test]
fn a_bad_top_level_declaration_is_reported() {
    // `!!!` lexes as three `!` tokens, none of which can start an item.
    assert_eq!(codes("!!!\n"), vec!["E1001"]);
}

#[test]
fn comparisons_do_not_chain() {
    // `a < b < c` is one comparison too many, and is reported rather than silently
    // parsed as `(a < b) < c`.
    assert!(!codes("label a:\n    var x = a < b < c\n").is_empty());
}

#[test]
fn diagnostics_are_reported_in_source_order() {
    let src = "label a:\n    var x = \n    var y = \n";
    let result = parse_src(src);
    let starts: Vec<u32> = result
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.primary.span.start())
        .collect();
    let mut sorted = starts.clone();
    sorted.sort_unstable();
    assert_eq!(starts, sorted);
}

#[test]
fn a_thousand_broken_lines_produce_one_diagnostic_each() {
    // The milestone's bar: a malformed file must produce a *bounded* number of
    // diagnostics. One per line is the definition of not cascading.
    let mut src = String::from("label a:\n");
    for _ in 0..1000 {
        src.push_str("    = 1\n");
    }

    let result = parse_src(&src);
    assert_eq!(result.diagnostics.len(), 1000);

    let Item::Label(label) = &result.program.items[0] else {
        panic!("expected a label");
    };
    assert_eq!(label.body.len(), 1000);
}

#[test]
fn e1004_reports_a_declaration_with_no_name() {
    assert!(codes("label\n").contains(&"E1004".to_string()));
}

#[test]
fn e1005_reports_a_missing_type() {
    let found = codes("label a:\n    var x: = 1\n");
    assert!(found.contains(&"E1005".to_string()), "{found:?}");
}

#[test]
fn an_effect_declaration_parses() {
    let program = parse_src(
        "effect rand.int(low: int, high: int) -> int\n\
         effect time.now() -> float\n\
         effect audio.play(channel: str, source: str)\n",
    );

    assert!(
        program.diagnostics.is_empty(),
        "{:?}",
        program
            .diagnostics
            .iter()
            .map(|d| format!("{}: {}", d.code.as_str(), d.message))
            .collect::<Vec<_>>()
    );

    let effects: Vec<&crate::tree::EffectDecl> = program
        .program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Effect(decl) => Some(decl),
            _ => None,
        })
        .collect();

    assert_eq!(effects.len(), 3);

    // The name is dotted, because effects are grouped by capability.
    assert_eq!(effects[0].dotted(), "rand.int");
    assert_eq!(effects[0].params.len(), 2);
    assert!(effects[0].ret.is_some());

    assert_eq!(effects[1].dotted(), "time.now");
    assert!(effects[1].params.is_empty());

    // An effect that returns nothing omits the arrow.
    assert_eq!(effects[2].dotted(), "audio.play");
    assert!(effects[2].ret.is_none());
}

/// Every path literal in a file is recorded, in source order, with its value and span.
#[test]
fn path_literals_are_recorded_as_the_file_is_parsed() {
    let parsed = parse_src(
        "image hero = @\"art/hero.png\"\n\nlabel start:\n    play music @\"audio/theme.ogg\"\n    \"Hi.\"\n    return\n",
    );
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let values: Vec<&str> = parsed
        .paths
        .iter()
        .map(|path| path.value.as_str())
        .collect();
    assert_eq!(values, vec!["art/hero.png", "audio/theme.ogg"]);

    // The span covers the literal as written, `@` and quotes included, so a diagnostic can
    // underline what the author typed rather than a trimmed form of it.
    let text = "image hero = @\"art/hero.png\"\n\nlabel start:\n    play music @\"audio/theme.ogg\"\n    \"Hi.\"\n    return\n";
    let first = &parsed.paths[0];
    assert_eq!(
        text.get(first.span.start() as usize..first.span.end() as usize),
        Some("@\"art/hero.png\"")
    );
}

/// A commented-out path is not a reference, which is what a scan for `@"` could not promise.
#[test]
fn a_path_in_a_comment_is_not_a_path() {
    let parsed =
        parse_src("# image hero = @\"art/hero.png\"\n\nlabel start:\n    \"Hi.\"\n    return\n");
    assert!(parsed.paths.is_empty(), "{:?}", parsed.paths);
}

/// A file with no path literals records none rather than inventing one.
#[test]
fn a_file_without_paths_records_none() {
    let parsed = parse_src("label start:\n    \"Hi.\"\n    return\n");
    assert!(parsed.paths.is_empty());
}
