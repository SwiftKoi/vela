//! What a line's arguments are: a value, or a name with one.
//!
//! `SCREENS.md §2`. The parser's one real ambiguity — `text line style = body` is a value and a name and a
//! value — decided by one-token lookahead rather than by the widget's schema, which the parser does not
//! have. Split from `screen_tests.rs` when a regression test for a reserved word's *call* pushed that file
//! past its budget, which is also the subject here.

use crate::tree::{Expr, Item, ScreenArg, ScreenLine};

use super::parse_tests::parse_src;

/// A call whose callee is a reserved word is a *call*, not two arguments.
///
/// `action jump(start)` is the case, and it was broken in a way nothing else could see. `jump` is a
/// statement at the start of a line and an action's name in expression position (`LANGUAGE.md §7.0`), and
/// the argument parser read the word after the line's name as another *argument's name* — `jump`, and a
/// parenthesised `start` — because it is not an identifier. Both halves of that parse, so the checker had
/// nothing to say and the widget held an action a *screen was given* rather than one it writes, which is
/// the shape a caller hands in: a button that silently did nothing, printed back by `vela fmt` as `action
/// jump = (start)` as if that meant the same thing.
#[test]
fn a_reserved_word_can_name_the_action_a_button_writes() {
    let source =
        "screen s:\n    button:\n        action jump(start)\n    button:\n        action quit()\n";
    let program = parse_src(source);
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
    // Both buttons' actions arrive the same way: one positional value, which is the call.
    for (index, callee) in [(0usize, "jump"), (1, "quit")] {
        let ScreenLine::Node(button) = &screen.body[index] else {
            panic!("expected a button at {index}");
        };
        let Some(ScreenArg::Value(Expr::Call { callee: found, .. })) =
            button.children.first().and_then(|line| match line {
                ScreenLine::Node(action) => action.args.first(),
                _ => None,
            })
        else {
            panic!("the action line is a call: {:#?}", button.children);
        };
        assert!(
            matches!(found.as_ref(), Expr::Name { name, .. } if name == callee),
            "{found:?}"
        );
    }
}

/// A number after a prop is an *expression* when the token before it is not `=`, and that is why a nudge
/// is two props rather than one pair of values.
///
/// Three spellings of the same thing — 40 to the right, 30 up — and the two that look natural are not
/// what they look like:
///
/// ```text
/// xoffset 40, yoffset = -30    two names, one value each — the safe spelling
/// xoffset 40, yoffset -30      `yoffset - 30` is one *value*, so the second prop never arrives
/// offset 40 -30                one prop whose value is the subtraction `40 - 30`
/// ```
///
/// `-` is both a binary and a unary operator, so `yoffset -30` is genuinely ambiguous and the parser
/// resolves it as arithmetic — faithfully, since `enable_if trust > 3` needs the same rule. A value that
/// is *data* rather than a computation is therefore written `name = value`, which is what `vela fmt`
/// canonicalizes to. A pair-of-numbers prop would have had no such spelling: `offset 40, -30` puts the
/// second number in an unnamed value, which nothing checks, so the comma it silently depends on is a
/// silent wrong answer when it is left out. Two props with one value each cannot have that failure
/// (`SCREENS.md §4.2`), and Ren'Py names them the same way.
#[test]
fn a_nudge_is_two_names_with_one_value_each() {
    let program = parse_src(
        "screen s:\n    box xoffset 40, yoffset = -30:\n        text \"a\"\n    box xoffset 40, yoffset -30:\n        text \"b\"\n    box offset 40 -30:\n        text \"c\"\n",
    );
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

    let ScreenLine::Node(pair) = &screen.body[0] else {
        panic!("expected the first box");
    };
    assert_eq!(pair.args.len(), 2, "one value each: {:?}", pair.args);
    for (index, expected) in [(0usize, "xoffset"), (1, "yoffset")] {
        assert!(
            matches!(&pair.args[index], ScreenArg::Named { name, value: Some(_), .. } if name == expected),
            "`{expected}` is a name with a value: {:?}",
            pair.args
        );
    }

    let ScreenLine::Node(unmarked) = &screen.body[1] else {
        panic!("expected the second box");
    };
    assert!(
        matches!(&unmarked.args[1], ScreenArg::Value(Expr::Binary { .. })),
        "`yoffset -30` is a subtraction, not a second prop: {:?}",
        unmarked.args
    );

    let ScreenLine::Node(one) = &screen.body[2] else {
        panic!("expected the third box");
    };
    assert_eq!(one.args.len(), 1, "one arg, not two: {:?}", one.args);
    assert!(
        matches!(&one.args[0], ScreenArg::Named { name, value: Some(Expr::Binary { .. }), .. } if name == "offset"),
        "`40 -30` is a subtraction: {:?}",
        one.args
    );
}
