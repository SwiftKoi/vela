//! What the pass could not write, gathered per screen and reported once per kind.
//!
//! The kinds are the *disposition*, not the construct: pixel placement is a skin's business, a
//! system is the host's, a picture is something a screen places, and the rest has no counterpart at
//! all. Grouping by kind rather than listing every line is what keeps the report a work list.

use super::words::distinct;
use super::*;

/// Everything the pass reports instead of translating.
#[derive(Default)]
pub(super) struct Gaps {
    /// Pixel placement, which a skin places.
    pub(super) placement: Vec<String>,
    /// Names with no counterpart at all.
    pub(super) unknown: Vec<String>,
    /// Pictures and other displayables, which a screen places.
    pub(super) pictures: Vec<String>,
    /// The systems met, in the order they were seen.
    pub(super) systems: Vec<&'static str>,
}

impl Gaps {
    /// Whether anything at all was left.
    pub(super) fn is_empty(&self) -> bool {
        self.placement.is_empty()
            && self.unknown.is_empty()
            && self.pictures.is_empty()
            && self.systems.is_empty()
    }

    /// One entry per kind of gap, so the report stays a work list rather than a transcript.
    pub(super) fn report(&self, relative: &str, name: &str, report: &mut Report) {
        for (names, phrasing, why) in [
            (
                &self.placement,
                "pixel placement",
                "a Vela layout places by `anchor` and `align` over named positions rather than by \
                 pixel coordinate (`SCREENS.md §4.2`), so a skin places these. A matched \
                 `xalign`/`yalign` pair came across as the position it names; the coordinates did not",
            ),
            (
                &self.pictures,
                "picture setting",
                "a Ren'Py style or widget can name a displayable — `Frame(\"gui/frame.png\")` — and a \
                 Vela style paints rather than places, so the screen that uses it adds the picture",
            ),
            (
                &self.unknown,
                "no counterpart",
                "these name something Vela expresses differently or not at all, so they are ported \
                 by hand",
            ),
        ] {
            if names.is_empty() {
                continue;
            }
            let names = distinct(names);
            report.push(
                relative,
                1,
                &format!(
                    "screens.rpy: `{name}`, {} {phrasing}(s): {}",
                    names.len(),
                    names.join(", ")
                ),
                why,
            );
        }
        for system in &self.systems {
            report.push(
                relative,
                1,
                &format!("screens.rpy: `{name}` uses {system}"),
                "that is a host system rather than a screen's own language, so it is reported and \
                 not translated",
            );
        }
    }
}
