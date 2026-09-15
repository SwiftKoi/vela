//! The one random number generator.
//!
//! `ARCHITECTURE.md §4`: RNG is *state*, not a service. It lives in `World` and advances
//! only when a story calls `rand`, so two runs with equal input logs have equal sequences
//! and a replay does not have to re-seed anything.
//!
//! SplitMix64, chosen for being four lines, having no state beyond a counter, and being
//! reproducible across platforms and versions — all of which matter more here than
//! statistical quality. This decides which of two dialogue lines a player sees.

use serde::{Deserialize, Serialize};

/// A seeded generator.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// A generator with a seed.
    #[must_use]
    pub fn seeded(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The next value.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A float in `[0, 1)`.
    ///
    /// Built from 53 bits, which is what a `f64` can hold exactly — taking fewer would make
    /// the distribution lumpy, and taking more would not survive the conversion.
    pub fn float(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// An integer in `[low, high]`, inclusive.
    ///
    /// Returns `low` for an empty range rather than faulting: a story asking for a number
    /// between 3 and 1 is a mistake, but it is not one worth stopping a game for.
    pub fn int(&mut self, low: i64, high: i64) -> i64 {
        if high <= low {
            return low;
        }
        let span = (high - low) as u64 + 1;
        low + (self.next_u64() % span) as i64
    }

    /// An index below `len`.
    #[must_use]
    pub fn pick(&mut self, len: usize) -> usize {
        if len == 0 {
            return 0;
        }
        (self.next_u64() % len as u64) as usize
    }
}

impl Default for Rng {
    /// A generator with a fixed seed, so that a `World` built without one still replays.
    fn default() -> Self {
        Self::seeded(0x5eed_1234_abcd_ef01)
    }
}
