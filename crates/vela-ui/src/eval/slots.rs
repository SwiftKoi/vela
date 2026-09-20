//! `slots(6)`: the question whose answer is a page of save slots.
//!
//! The third question a host answers (`SCREENS.md §2.6`) and the first whose answer is a *list* —
//! `variant(…)` answers where the screen is running, `setting(…)` answers what the player chose, and this
//! answers what is in each cell of a page. A save screen's whole body is a loop over it, so the shape has to
//! be the one a `for` walks and a field reads (`§2.4`): a list of records, one per cell, empty ones too.
//!
//! The page comes from the player's own setting rather than from the call, because that is what a page is
//! (`crate::slots`, `RUNTIME.md §2.1`); the *count* is the caller's, because a grid is a layout.

use crate::slots;
use crate::value::{Args, Value};

use super::value_of;

/// The name a screen asks the slots with.
pub(crate) const SLOTS: &str = "slots";

/// Whether this word is the slots question.
pub(crate) fn is_question(name: &str) -> bool {
    name == SLOTS
}

/// The answer to a `slots(count)` call.
///
/// A count that is not a number, or one that is not positive, answers the empty list: a screen that wrote
/// `slots(none)` is asking for nothing, and the checker is what tells its author that the question cannot
/// mean anything — the total-evaluator rule `variant_answer` and `setting`'s answer both follow.
pub(super) fn answer(args: &[vela_syntax::Expr], values: &Args) -> Value {
    let [count, ..] = args else {
        return Value::None;
    };
    let Ok(count) = u32::try_from(f64::from(number(count, values)).max(0.0) as u64) else {
        return Value::List(Vec::new());
    };
    Value::List(slots::page(
        values.slots(),
        crate::settings::page(values.preferences()),
        count,
    ))
}

/// The count the call was given, as a number.
fn number(expr: &vela_syntax::Expr, values: &Args) -> f32 {
    match value_of(expr, values) {
        Value::Num(count) => count as f32,
        _ => 0.0,
    }
}
