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
