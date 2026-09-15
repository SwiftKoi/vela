//! The migration chain this build ships, in one place.
//!
//! `CONVENTIONS.md §4.9`: a version step is a new file plus one line here. The chain is built
//! once per load and building it *validates* it — a duplicate `from` or a skipped version is
//! a bug in this table, not in a player's save, so it is caught before anything is read.

use crate::migrations::chain::Migrator;
use crate::migrations::v0002_trust_to_affection;
use crate::migrations::v0003_frame_anchor;

/// Every migration, ordered by the version it starts from.
///
/// The `expect` is a genuine bug rather than user input: the list below is authored, and
/// `the_shipped_chain_covers_every_version` fails first if it is ever wrong.
#[must_use]
pub fn chain() -> Migrator {
    Migrator::new(vec![
        v0002_trust_to_affection::migration(),
        v0003_frame_anchor::migration(),
    ])
    .expect("the shipped migration chain advances exactly one version at a time")
}
