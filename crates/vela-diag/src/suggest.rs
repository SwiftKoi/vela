//! Suggesting what someone probably meant.
//!
//! Here rather than in a consumer because *two* consumers need it — a name resolver matching a
//! label, and a widget registry matching an `E5006` widget name — and a second copy is a second
//! threshold to tune.
//!
//! A typo in a label name is the most common mistake an author makes, and the position alone
//! does not help: `undefined label `forest.clearring`` tells you where the mistake is and
//! nothing about what it should have been. Offering the nearest name turns a hunt into a
//! fix.
//!
//! The threshold is deliberately conservative. A wrong suggestion is worse than none — it
//! sends someone looking in the wrong place — so this offers a candidate only when it is
//! genuinely close, and stays quiet otherwise.

/// The candidate closest to `name`, when one is close enough to be worth offering.
#[must_use]
pub fn closest<'a>(name: &str, candidates: impl Iterator<Item = &'a str>) -> Option<String> {
    let mut best: Option<(usize, &str)> = None;
    for candidate in candidates {
        let distance = distance(name, candidate);
        if best.is_none_or(|(nearest, _)| distance < nearest) {
            best = Some((distance, candidate));
        }
    }

    let (distance, candidate) = best?;

    // Scales with the name and stops at three: `clearring`/`clearing` is one edit, and a
    // four-edit suggestion for a short name is a different label, not a typo.
    let limit = (name.chars().count() / 3).clamp(1, 3);
    (distance <= limit).then(|| candidate.to_string())
}

/// Levenshtein distance, over characters rather than bytes.
///
/// Two rows rather than a matrix: the names involved are short, but an O(n·m) allocation per
/// candidate is the kind of thing that quietly costs a language server its responsiveness.
#[must_use]
pub fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();

    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }

    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current: Vec<usize> = vec![0; b.len() + 1];

    for (i, left) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, right) in b.iter().enumerate() {
            let substitution = previous[j] + usize::from(left != right);
            current[j + 1] = substitution.min(previous[j + 1] + 1).min(current[j] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }

    previous[b.len()]
}
