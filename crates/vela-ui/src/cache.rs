//! Not laying out what did not change.
//!
//! `SCREENS.md §8.3`: *"a widget repaints when its inputs change or when the animation clock
//! advances it. There is no 'repaint everything' mode. This is a defined property, not an
//! optimization: it makes frame cost predictable."*
//!
//! So this is not a cache in the "make it faster" sense. A dialogue screen at 60fps re-running
//! layout over the whole tree every frame is not slow, it is *unbounded* — its cost grows with
//! the story rather than with what changed, and a scene with a busy screen would drop frames
//! on a machine where the same scene with a quiet one would not.
//!
//! The cache holds one screen's laid-out frame and the dependency set it was built from. A
//! frame is reused when nothing it reads has changed, which for a static screen is always.

use crate::deps::{DepSet, deps_of};
use crate::layout::Frame;
use crate::tree::Node;
use vela_syntax::ScreenLine;

/// A laid-out screen, and what it was laid out from.
#[derive(Debug)]
pub struct Cached {
    /// The frame.
    pub frame: Frame,
    /// What it reads.
    pub deps: DepSet,
}

/// A screen's last layout.
#[derive(Debug, Default)]
pub struct ScreenCache {
    entry: Option<Cached>,
    /// How many times a layout was actually run. The measurement the exit criterion asks for:
    /// *"a benchmark proves a static screen does not relayout when unrelated state changes"*.
    layouts: usize,
}

impl ScreenCache {
    /// An empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The last frame, if nothing it reads has changed.
    ///
    /// Returns the frame and whether it was reused, so a caller — and a test — can tell the two
    /// apart rather than inferring it from a count.
    pub fn get(&self, changed: &[String]) -> Option<(&Frame, bool)> {
        let entry = self.entry.as_ref()?;
        if entry.deps.is_hit_by(changed) {
            return None;
        }
        Some((&entry.frame, true))
    }

    /// The last frame regardless of changes, for a caller that has already decided.
    #[must_use]
    pub fn frame(&self) -> Option<&Frame> {
        self.entry.as_ref().map(|entry| &entry.frame)
    }

    /// The dependency set the cached frame was built from.
    #[must_use]
    pub fn deps(&self) -> Option<&DepSet> {
        self.entry.as_ref().map(|entry| &entry.deps)
    }

    /// Builds a layout if the cache does not already answer, and returns the frame.
    ///
    /// `build` runs only when it has to, which is what makes the count meaningful: a caller
    /// that laid out every frame would see `layouts` climb whether or not anything changed.
    pub fn get_or_build(
        &mut self,
        body: &[ScreenLine],
        node: &Node,
        constraints: crate::layout::Constraints,
        changed: &[String],
        build: impl FnOnce() -> Frame,
    ) -> &Frame {
        let stale = self
            .entry
            .as_ref()
            .is_none_or(|entry| entry.deps.is_hit_by(changed));
        if stale {
            let frame = build();
            self.entry = Some(Cached {
                frame,
                deps: deps_of(body),
            });
            self.layouts += 1;
        }
        let _ = (node, constraints);
        &self.entry.as_ref().expect("just built").frame
    }

    /// How many layouts have been run.
    #[must_use]
    pub fn layouts(&self) -> usize {
        self.layouts
    }

    /// Forgets the layout, as a resize or a hot reload does.
    pub fn invalidate(&mut self) {
        self.entry = None;
    }
}
