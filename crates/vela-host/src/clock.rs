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

/// A stamp as a player reads it: `2026-09-20 12:34`, in UTC.
///
/// The other half of [`stamp`], and here for the same reason: showing a time is the platform's business —
/// a *locale* and a *time zone* are two more things the engine would have to know about a machine (`TZ`,
/// `LC_TIME`, a Windows registry) — and the alternative, a date library, is a dependency for one layout.
///
/// UTC rather than local, and one fixed layout rather than a locale's: a save menu's "when" is a *label*
/// beside a slot's name, not a contract, and a label that is the same on every machine is the one a test
/// can assert. A time zone the player cares about is a setting nobody has asked for yet.
#[must_use]
pub fn format_time(stamp: u64) -> String {
    let seconds = i64::try_from(stamp).unwrap_or(i64::MAX);
    let (days, time) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        time / 3600,
        (time % 3600) / 60
    )
}

/// The civil date a count of days since 1970-01-01 names.
///
/// Howard Hinnant's `civil_from_days`, which is how this is done without a library: the era is a 400-year
/// cycle of exactly 146 097 days, and the year inside it is counted from *March* so a leap day lands at the
/// end rather than in the middle.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let march = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * march + 2) / 5 + 1) as u32;
    let month = if march < 10 { march + 3 } else { march - 9 } as u32;
    (year + i64::from(month <= 2), month, day)
}

#[cfg(test)]
mod tests {
    use super::{format_time, stamp};

    /// A time a player reads, in one layout, whatever the machine's zone and locale are.
    ///
    /// `1_700_000_000` is 2023-11-14 22:13:20 UTC, which is the kind of number a slot carries and the sort
    /// of thing a date library would be pulled in for. The epoch and a day's worth of seconds are the ends of
    /// it: a day is 86 400, and a formatter that had seconds and milliseconds confused would say 1970 still.
    #[test]
    fn a_stamp_formats_as_a_time_a_player_reads() {
        assert_eq!(format_time(0), "1970-01-01 00:00");
        assert_eq!(format_time(86_400), "1970-01-02 00:00");
        assert_eq!(format_time(1_700_000_000), "2023-11-14 22:13");
        // A leap day, because that is what the March-based year inside the era is for:
        assert_eq!(format_time(1_582_934_400), "2020-02-29 00:00");
    }

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
