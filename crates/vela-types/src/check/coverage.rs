//! Whether a `match` covers every case.
//!
//! One answer, asked twice: `E4001` reports the arms a match is missing, and the return analysis
//! asks whether a match that exits in every arm is a path that always exits. Two implementations
//! of "covered" would agree until the first one was fixed, and then the checker would report a
//! function for not returning while accepting the very match it reports on — which is exactly the
//! false positive this module exists to prevent.

use std::collections::BTreeSet;

use vela_syntax::MatchArm;

use crate::env::Env;
use crate::ty::Ty;

/// The variants a match leaves uncovered. Empty means every case is covered.
///
/// Empty is also the answer when nothing can be missed: only an enum has cases, so a match over
/// anything else is covered by whatever arms are written, and reasoning further would need more
/// of the language than exists yet.
pub(crate) fn missing(env: &Env, scrutinee: &Ty, arms: &[MatchArm]) -> Vec<String> {
    if arms.iter().any(open) {
        return Vec::new();
    }
    let Ty::Enum(name) = scrutinee else {
        return Vec::new();
    };
    let Some(shape) = env.enum_shape(name) else {
        return Vec::new();
    };

    let covered: BTreeSet<&str> = arms.iter().filter_map(variant).collect();
    shape
        .variants
        .keys()
        .filter(|candidate| !covered.contains(candidate.as_str()))
        .cloned()
        .collect()
}

/// Whether an arm covers whatever is left: `else`, or the `_` wildcard.
fn open(arm: &MatchArm) -> bool {
    match &arm.pattern {
        None => true,
        Some(pattern) => pattern.path.is_empty(),
    }
}

/// The variant an arm names, if it names one.
fn variant(arm: &MatchArm) -> Option<&str> {
    let pattern = arm.pattern.as_ref()?;
    pattern.path.last().map(String::as_str)
}
