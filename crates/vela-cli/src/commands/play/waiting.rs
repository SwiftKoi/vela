//! What a run waits for, which is what a player's keypresses land on.
//!
//! Its own file rather than a function in `play.rs` for the reason the file budget exists: this is a
//! *policy* — which commands hold the story — and the player below it is the machinery that applies
//! it. The visual check that found the bug behind it is `TOOLING.md §1.1`.

/// Whether a command holds the story until the player, or a clock, moves it on.
///
/// A line, a choice and a click-wait are things a player reads or answers. A `pause 2.0` is a
/// clock's, not a player's — Ren'Py advances it on its own — so it does not wait for a key either,
/// and a `pause` with no duration is a click like any other.
pub(crate) fn waits_for_the_player(command: &vela_world::Command) -> bool {
    match command {
        vela_world::Command::Say { .. } | vela_world::Command::Menu { .. } => true,
        vela_world::Command::WaitClick => true,
        vela_world::Command::Pause { seconds } => seconds.is_none(),
        _ => false,
    }
}
