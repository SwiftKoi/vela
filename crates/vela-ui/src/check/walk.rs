//! Descending into the bodies a line holds.
//!
//! [`ScreenLine::bodies`] answers for an `if`'s arms and a `for`'s body, and deliberately not for a
//! `use` block or a widget's children — a walker that recursed through it would skip its own work on
//! those two, because both carry something besides a body. A rule that has *no* work of its own to skip
//! wants the whole set, and two of them do (`variables` and `conditions`), so it is answered once here.

use vela_syntax::ScreenLine;

/// Every body a line holds: an arm, a loop, a `use` block, or a widget's children.
pub(super) fn nested(line: &ScreenLine) -> Vec<&[ScreenLine]> {
    match line {
        ScreenLine::Use { body, .. } => vec![body],
        ScreenLine::Node(node) => vec![&node.children],
        other => other.bodies(),
    }
}
