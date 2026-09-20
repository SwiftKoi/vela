//! The platform clock: the one reading of wall time in the engine.
//!
//! `check-determinism` bans `SystemTime::now` everywhere else, and the exemption's justification is the
//! rule this module implements: *the host owns the platform clock, and every other crate takes time as
//! an injected value* (`RUNTIME.md §4.2`). So a crate that needs a stamp for something a player will
//! *see* — a save's `created_at`, a log line — asks for one here and passes it down.
//!
//! Nothing a *story* does can reach this, which is what keeps the engine replayable: a save's stamp is
//! written beside the world rather than into it (`RUNTIME.md §5`), so two runs of the same story still
//! produce the same commands, the same snapshot, and the same bytes.

use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds since the Unix epoch, as the platform reports them.
///
/// Seconds, not milliseconds or nanoseconds, because the value is written into a file that outlives the
/// process: a save's stamp has to mean the same thing to every build that reads it back, and a
/// sub-second precision would be a promise about a clock that no player asked for.
///
/// A platform whose clock is set before 1970 answers `0` rather than failing: the value is display-only,
/// and a save that was written is worth more than a save refused over its label.
#[must_use]
pub fn stamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::stamp;

    /// The unit is part of the contract: a stamp in milliseconds reads as a time forty thousand years
    /// from now and a stamp in seconds reads as this century, so the assertion is a range rather than a
    /// value — `RUNTIME.md §5`'s `created_at` is compared and displayed, never computed with.
    #[test]
    fn a_stamp_is_a_unix_time_in_seconds() {
        // 2020-01-01, and the year 2200: anything between them is a plausible *seconds* stamp, and a
        // millisecond or nanosecond reading of the same instant is far outside it.
        let now = stamp();
        assert!(
            (1_577_836_800..4_102_444_800).contains(&now),
            "the platform clock is not reporting seconds since the epoch: {now}"
        );
    }
}
