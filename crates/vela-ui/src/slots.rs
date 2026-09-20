//! The slots a screen is offered, and what it means to ask about them (`SCREENS.md §7`).
//!
//! The third question a host answers (`§2.6`), and the first that answers a **list**: a save screen draws a
//! page of slots, so what it needs is not one value but one per cell — the number to act on, whether
//! anything is there, and what to draw for it. What a slot *is* belongs to the store (`vela-replay`: a file
//! with metadata); this is the shape a screen reads, which is why it is here rather than beside the file.
//!
//! **The page is the player's, not an argument.** `slots(count)` answers the page `setting("file_page")`
//! names, because that is what a page *is*: one position the player is looking at (`RUNTIME.md §2.1`), moved
//! by the page actions, and a page the player is not on is not a page a screen is drawing. The **count** is
//! an argument, because how many cells a page has is a layout decision — a screen with a 3×2 grid asks for
//! six — and the store has no opinion about it.

use crate::value::Value;

/// One slot a screen may draw, as the host found it.
///
/// The host reads its own store and hands these over; everything below is about *reading* them, which is
/// the half a screen can see.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Slot {
    /// The page it is on, as the page actions count them.
    pub page: u32,
    /// Its number within the page: what `file_action(n)` takes.
    pub number: u32,
    /// The name it is filed under (`1-3`), which is what a delete or a migration message says.
    pub name: String,
    /// When it was written, seconds since the epoch: `RUNTIME.md §5`'s `created_at`, kept raw.
    pub time: u64,
    /// The same time as a player reads it — `2026-09-20 12:34` — formatted by the *host*, because a locale
    /// and a time zone are the platform's business (`vela_host::format_time`) and a screen has no date type.
    ///
    /// Carried beside the raw stamp rather than instead of it: a screen draws this one, and a test or a
    /// comparison keeps the number that means something.
    pub when: String,
    /// Whether this build can load it: a version from the future or a half-written file answers false.
    pub loadable: bool,
}

/// What a `slots(count)` call answers: one record per cell of the player's page, empty ones included.
///
/// Empty cells are the point rather than a convenience. A *save* screen draws them so they can be pressed,
/// so an answer that carried only the occupied slots would leave that screen with no grid to draw — and a
/// screen cannot ask about a slot it was not offered. Every number from one to `count` appears exactly once,
/// in order, so a screen can index the answer by position if it wants to.
#[must_use]
pub fn page(slots: &[Slot], page: u32, count: u32) -> Vec<Value> {
    (1..=count)
        .map(|number| match find(slots, page, number) {
            Some(slot) => drawn(slot),
            None => empty(number),
        })
        .collect()
}

/// The slot at a page and number, if the host has one there.
fn find(slots: &[Slot], page: u32, number: u32) -> Option<&Slot> {
    slots
        .iter()
        .find(|slot| slot.page == page && slot.number == number)
}

/// A record for a slot that holds something.
fn drawn(slot: &Slot) -> Value {
    record(
        slot.number,
        (slot.name.clone(), slot.when.clone()),
        slot.time,
        slot.loadable,
        false,
    )
}

/// A record for a cell with nothing in it.
///
/// It carries its *number*, because that is what a save screen's button acts on: "save into slot three" is a
/// press on an empty cell, and a record with no number would leave the control with nothing to say.
fn empty(number: u32) -> Value {
    record(number, (String::new(), String::new()), 0, false, true)
}

/// One record, with the fields in one order for every cell.
fn record(number: u32, text: (String, String), time: u64, loadable: bool, empty: bool) -> Value {
    let (name, when) = text;
    Value::Record(vec![
        ("number".to_string(), Value::Num(f64::from(number))),
        ("name".to_string(), Value::Str(name)),
        ("when".to_string(), Value::Str(when)),
        ("time".to_string(), Value::Num(time as f64)),
        ("loadable".to_string(), Value::Bool(loadable)),
        ("empty".to_string(), Value::Bool(empty)),
    ])
}
