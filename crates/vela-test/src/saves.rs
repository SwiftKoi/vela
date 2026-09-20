//! The slots a test run may touch: a directory of its own, and the schema its worlds are written against.
//!
//! `RUNTIME.md §5`. A run carries out the file actions a screen asks for (`SCREENS.md §7`), and it carries
//! them out against slots of its own: a test that wrote into the saves a person plays from would be a test
//! that deletes their progress. One directory per *test*, because two tests in one file share a run — an
//! assertion about an empty page must not be broken by whatever test ran before it.
//!
//! Both halves come from the caller, and each for its own reason: the schema is derived from the source a
//! program was compiled from, so only the layer that has a project can build it, and the root is the
//! caller's to place and to clean up (the CLI makes one per run, under the system's temporary directory).

use std::path::PathBuf;

use vela_replay::Schema;

/// Where a run's slots live, and what their worlds are checked against.
#[derive(Clone, Debug)]
pub struct Saves {
    root: PathBuf,
    schema: Schema,
}

impl Saves {
    /// A root directory and the schema to write against.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>, schema: Schema) -> Self {
        Self {
            root: root.into(),
            schema,
        }
    }

    /// The same slots for one test: a directory of its own under the root.
    ///
    /// A path rather than a directory that is made here: a slot's directory appears when one is first
    /// written, and a test that only reads finds nothing — which is the same answer
    /// `vela_replay::slots` gives for a directory that is not there, and the state a save screen has to
    /// draw first.
    #[must_use]
    pub fn of(&self, test: usize) -> Self {
        Self {
            root: self.root.join(test.to_string()),
            schema: self.schema.clone(),
        }
    }

    /// The directory this test's slots live in.
    #[must_use]
    pub fn dir(&self) -> &PathBuf {
        &self.root
    }

    /// The schema a slot is written and read against (`RUNTIME.md §5`).
    #[must_use]
    pub fn schema(&self) -> &Schema {
        &self.schema
    }
}
