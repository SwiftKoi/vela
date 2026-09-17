//! Noticing that a screen's source changed.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Notices when any of a set of files changes, by modification time.
///
/// No clock is read: a stamp is compared with the last one seen, so this cannot be a source of
/// nondeterminism even though the values it compares are times. It polls rather than using an
/// OS watcher, because a watcher library is a dependency and a platform surface for a handful
/// of files — and the idle tick that drives it already exists.
pub struct Watcher {
    stamps: Vec<(PathBuf, Option<SystemTime>)>,
}

impl Watcher {
    /// Remembers the current stamp of every file.
    #[must_use]
    pub fn new(paths: &[PathBuf]) -> Self {
        Self {
            stamps: paths
                .iter()
                .map(|path| (path.clone(), stamp(path)))
                .collect(),
        }
    }

    /// Whether any file changed since the last call, updating the stamps.
    pub fn changed(&mut self) -> bool {
        let mut changed = false;
        for (path, last) in &mut self.stamps {
            let current = stamp(path);
            if *last != current {
                *last = current;
                changed = true;
            }
        }
        changed
    }
}

/// A file's modification time, or `None` if it cannot be read.
fn stamp(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|meta| meta.modified()).ok()
}
