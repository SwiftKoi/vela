//! Parsing `screen` bodies into widget trees.
//!
//! `SCREENS.md §2`. Split from `parse_tests.rs` when that file crossed the size budget, along
//! the boundary that was already there: this is the one construct in the language whose lines
//! are *ambiguous by design*, and every test here is about that.

use vela_span::FileId;

use crate::tree::{Expr, Item, ScreenArg, ScreenLine};

use super::parse_tests::parse_src;

/// A parameter may be written without a type, and the printer round-trips it.
///
/// `param = IDENT [ ":" type ]` (`LANGUAGE.md §3`). An absent type is the checker's `Ty::Unknown`
/// (`§5.4`) — the shape a migrated screen's parameters arrive in — so the parser, the tree, and the
/// printer all have to agree that "no colon" is a parameter and not an error.
#[test]
fn an_untyped_parameter_round_trips() {
    let source = "screen s(prompt, title: str, show = false):\n    text prompt\n";
    let program = parse_src(source);
    assert!(
        program.diagnostics.is_empty(),
        "{:?}",
        program
            .diagnostics
            .iter()
            .map(|d| format!("{}: {}", d.code.as_str(), d.message))
            .collect::<Vec<_>>()
    );

    let Some(Item::Screen(screen)) = program.program.items.first() else {
        panic!("expected a screen");
    };
    assert_eq!(screen.params.len(), 3, "the parameter list");
    assert!(
        screen.params[0].ty.is_none(),
        "`prompt` has no written type"
    );
    assert!(screen.params[1].ty.is_some(), "`title` is typed");
    assert!(
        screen.params[2].ty.is_none(),
        "`show` has no written type but a default"
    );

    // The canonical form writes a type back only where one was written, and re-parses to the same
    // absence of one — without this the formatter would invent a type on every pass.
    let formatted = crate::format(FileId::from_raw(0), source).expect("the screen formats");
    assert!(
        formatted.contains("screen s(prompt, title: str, show = false):"),
        "{formatted}"
    );

    let reparsed = parse_src(&formatted);
    let Some(Item::Screen(screen)) = reparsed.program.items.first() else {
        panic!("expected a screen");
    };
    assert!(screen.params[0].ty.is_none(), "`prompt` lost its absence");
    assert!(screen.params[1].ty.is_some(), "`title` lost its type");
    assert!(screen.params[2].ty.is_none(), "`show` lost its absence");
}

#[test]
fn a_screen_body_parses_into_a_widget_tree() {
    let source = "\
screen dialogue(name: str?, line: str, show_choices: bool = false):
    layer ui
    box at bottom, stretch_x:
        pad 24
        column gap 8:
            if show_choices:
                text name style = speaker
            text line style = body
";
    let program = parse_src(source);
    assert!(
        program.diagnostics.is_empty(),
        "{:?}",
        program
            .diagnostics
            .iter()
            .map(|d| format!("{}: {}", d.code.as_str(), d.message))
            .collect::<Vec<_>>()
    );

    let Some(Item::Screen(screen)) = program.program.items.first() else {
        panic!("expected a screen");
    };
    assert_eq!(screen.name, "dialogue");
    assert_eq!(screen.params.len(), 3, "the parameter list");
    assert_eq!(screen.body.len(), 2, "a layer and a box");

    // `layer ui`
    let ScreenLine::Layer { name, .. } = &screen.body[0] else {
        panic!("expected a layer, got {:?}", screen.body[0]);
    };
    assert_eq!(name, "ui");

    // `box at bottom, stretch_x:` with two lines in it.
    let ScreenLine::Node(box_node) = &screen.body[1] else {
        panic!("expected a box");
    };
    assert_eq!(box_node.name, "box");
    assert_eq!(box_node.args.len(), 2, "at bottom, stretch_x");
    let ScreenArg::Named { name, value, .. } = &box_node.args[0] else {
        panic!("expected a named arg");
    };
    assert_eq!(name, "at");
    assert!(value.is_some(), "`at bottom` takes a value");
    let ScreenArg::Named { name, value, .. } = &box_node.args[1] else {
        panic!("expected a named arg");
    };
    assert_eq!(name, "stretch_x");
    assert!(value.is_none(), "a bare word is a flag, not a value");
    assert_eq!(box_node.children.len(), 2, "pad and column");

    let ScreenLine::Node(column) = &box_node.children[1] else {
        panic!("expected a column");
    };
    assert_eq!(column.name, "column");
    let ScreenArg::Named { name, .. } = &column.args[0] else {
        panic!("expected a named arg");
    };
    assert_eq!(name, "gap");
    assert_eq!(column.children.len(), 2, "an if and a text");

    let ScreenLine::If { body, .. } = &column.children[0] else {
        panic!("expected an if");
    };
    assert_eq!(body.len(), 1, "text name style = speaker");

    let ScreenLine::Node(text) = &column.children[1] else {
        panic!("expected a text");
    };
    assert_eq!(text.name, "text");
    assert_eq!(text.args.len(), 2, "line, style = body");
    // `line` is a bare identifier, so it parses as a name with no value — and `"hi"` in
    // `text "hi"` parses as a value. Both are the text widget's content, and telling them
    // apart needs the schema, which the parser does not have. The checker resolves it.
    assert!(matches!(
        &text.args[0],
        ScreenArg::Named { name, value: None, .. } if name == "line"
    ));
    assert!(matches!(
        &text.args[1],
        ScreenArg::Named { value: Some(_), .. }
    ));
}

/// A prop written on its own line is a node with no children — `pad 24` and `column gap 8:`
/// differ only by the colon, which is the parser's whole job here.
#[test]
fn a_bare_prop_line_is_a_node_without_children() {
    let program = parse_src("screen s:\n    box:\n        pad 24\n");
    let Some(Item::Screen(screen)) = program.program.items.first() else {
        panic!("expected a screen");
    };
    let ScreenLine::Node(box_node) = &screen.body[0] else {
        panic!("expected a box");
    };
    let ScreenLine::Node(pad) = &box_node.children[0] else {
        panic!("expected a pad line");
    };
    assert_eq!(pad.name, "pad");
    assert!(!pad.has_children());
    assert!(
        matches!(&pad.args[0], ScreenArg::Value(_)),
        "`pad 24` is a name and a bare value, exactly as `text \"hi\"` is"
    );
}

/// An empty body is a screen with nothing in it, not a parse error.
#[test]
fn an_empty_screen_body_parses() {
    let program = parse_src("screen s:\n    pass\n");
    assert!(program.diagnostics.is_empty());
}

/// An action with no arguments is a call, not a parenthesised expression.
///
/// `action quit()` puts a call where a prop's value goes. Reading `quit` as the arg's *name*
/// leaves `()` to be parsed as a parenthesised expression, which fails at the `)` — and it
/// failed only in a screen, so `quit()` parsed everywhere else and the bug looked like
/// something else entirely.
#[test]
fn an_empty_argument_list_parses_as_a_call() {
    let program = parse_src("screen s:\n    button:\n        action quit()\n");
    assert!(
        program.diagnostics.is_empty(),
        "{:?}",
        program
            .diagnostics
            .iter()
            .map(|d| d.message.clone())
            .collect::<Vec<_>>()
    );

    let Some(Item::Screen(screen)) = program.program.items.first() else {
        panic!("expected a screen");
    };
    let ScreenLine::Node(button) = &screen.body[0] else {
        panic!("expected a button");
    };
    let ScreenLine::Node(action) = &button.children[0] else {
        panic!("expected an action line");
    };
    assert_eq!(action.name, "action");
    // `action quit()` is a name and a bare value — the same shape as `pad 24` and
    // `text "hi"`. Which of them is a widget and which a prop is the checker's question.
    let ScreenArg::Value(value) = &action.args[0] else {
        panic!("expected a bare value, got {:?}", action.args[0]);
    };
    assert!(
        matches!(value, Expr::Call { args, .. } if args.is_empty()),
        "expected an empty call, got {value:?}"
    );
}

/// Where a name is *declared*, a reserved word is a name.
///
/// Four positions were fixed one at a time before anyone noticed they were one bug: an effect
/// called `audio.play`, a widget called `image`, a screen called `pause`, and a theme token
/// read as `theme.fg`. Every one was a word the language reserves in *some* position and a
/// perfectly good name in another.
#[test]
fn reserved_words_are_names_where_a_name_is_declared() {
    let program = parse_src(
        "struct image:\n    field: int\n\nenum return:\n    a\n\nfn guard(a: int) -> int:\n    return a\n\nlabel pause:\n    return\n",
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
}

/// A reserved word is read like any other name — the boundary this used to assert is gone.
///
/// It said *"if this now parses, the boundary has moved and this test should say how"*, and
/// then it did. `LANGUAGE.md §7.0` is the how: a keyword is special at the start of a line and
/// a name everywhere else, so an expression is no longer an exception.
#[test]
fn a_reserved_word_is_a_value_reference() {
    let program = parse_src("label a:\n    var x = scene\n    var y = menu\n    return\n");
    assert!(
        program.diagnostics.is_empty(),
        "{:?}",
        program
            .diagnostics
            .iter()
            .map(|d| format!("{}: {}", d.code.as_str(), d.message))
            .collect::<Vec<_>>()
    );
}

/// An `if`'s `elif`s and its `else` are arms of one line, not lines beside it.
///
/// The shape is the statement form's (`IfStmt`, `LANGUAGE.md §3`), and it is what keeps a conditional
/// one node: the arms belong to one another, so a stray `else` is not something the grammar can
/// produce, and choosing an arm is a single decision rather than a walk that has to remember what the
/// line before it concluded.
#[test]
fn an_if_chain_is_one_line_with_its_arms() {
    let program = parse_src(
        "screen s:\n    if a:\n        text \"A\"\n    elif b:\n        text \"B\"\n    elif c:\n        text \"C\"\n    else:\n        pass\n",
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

    let Some(Item::Screen(screen)) = program.program.items.first() else {
        panic!("expected a screen");
    };
    assert_eq!(screen.body.len(), 1, "the chain is one line, not five");

    let ScreenLine::If {
        body,
        elifs,
        else_body,
        ..
    } = &screen.body[0]
    else {
        panic!("expected an `if`");
    };
    assert_eq!(body.len(), 1, "the `then` arm");
    assert_eq!(elifs.len(), 2, "two `elif` arms");
    assert!(else_body.is_some(), "and the `else` is the same line's");
}
