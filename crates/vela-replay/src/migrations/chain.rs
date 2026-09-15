//! The migration chain: ordered steps that rewrite an old world into the current shape.
//!
//! `RUNTIME.md §6.2`. A save records the `save_version` it was written at; the chain holds
//! one migration per version step, and a load applies them in order until the world is
//! current. The alternative — a decoder per release — is the same work with no place to say
//! *what changed*, which is the part a reader needs.
//!
//! A step is declarative rather than a function over raw bytes: `rename_field`,
//! `add_default`, and `transform`. That is deliberate. A migration is the highest-risk code
//! in the engine — it runs on the one artifact a player cannot regenerate — so the common
//! cases are named and reviewable, and the escape hatch is a single `fn(&mut World)`.

use std::fmt;

use vela_world::{Value, World};

use crate::error::ReplayError;
use crate::schema::Schema;

/// Why a set of migrations is not a usable chain.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ChainError {
    /// Two migrations start from the same version.
    Duplicate {
        /// The version both of them start from.
        from: u16,
    },
    /// A migration does not advance by exactly one version.
    NotAdjacent {
        /// The version it starts from.
        from: u16,
        /// The version it claims to produce.
        to: u16,
    },
}

impl fmt::Display for ChainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Duplicate { from } => {
                write!(f, "two migrations both start from save version {from}")
            }
            Self::NotAdjacent { from, to } => write!(
                f,
                "a migration goes from save version {from} to {to}, which skips a version"
            ),
        }
    }
}

impl std::error::Error for ChainError {}

/// One operation a migration performs on a world.
#[derive(Clone, Debug)]
enum Step {
    /// Move a value to a new name.
    RenameField {
        /// The name the value had.
        from: String,
        /// The name it takes.
        to: String,
    },
    /// Give a value older saves lack its current default.
    AddDefault {
        /// The default's name.
        name: String,
        /// The value to seed.
        value: Value,
    },
    /// An arbitrary rewrite.
    Transform(fn(&mut World)),
}

/// One version step: what brings a world from `from` to `to`.
#[derive(Clone, Debug)]
pub struct Migration {
    from: u16,
    to: u16,
    steps: Vec<Step>,
}

impl Migration {
    /// A migration from `from` to `to`, with no operations yet.
    ///
    /// Usually built by the [`migration!`](crate::migration) macro rather than by hand.
    #[must_use]
    pub fn new(from: u16, to: u16) -> Self {
        Self {
            from,
            to,
            steps: Vec::new(),
        }
    }

    /// Moves a saved value from one name to another.
    ///
    /// A no-op when the old name is absent: a save only holds the defaults its story has
    /// touched, and a project that never had the old field is not an error.
    #[must_use]
    pub fn rename_field(mut self, from: impl Into<String>, to: impl Into<String>) -> Self {
        self.steps.push(Step::RenameField {
            from: from.into(),
            to: to.into(),
        });
        self
    }

    /// Gives a value that older saves lack its current default.
    ///
    /// Applied only when the *current* schema declares a `default` of this name. A chain is
    /// shared by every project, so setting a field no project declares would write state its
    /// story never asked for — and the field would then ride along in every later save.
    #[must_use]
    pub fn add_default(mut self, name: impl Into<String>, value: Value) -> Self {
        self.steps.push(Step::AddDefault {
            name: name.into(),
            value,
        });
        self
    }

    /// Runs an arbitrary rewrite, for what the named operations cannot express.
    ///
    /// A plain `fn` rather than a closure: a migration is a fixed, named transformation, and
    /// one that cannot capture the state around it cannot depend on when it ran.
    #[must_use]
    pub fn transform(mut self, rewrite: fn(&mut World)) -> Self {
        self.steps.push(Step::Transform(rewrite));
        self
    }

    /// The version the migration starts from.
    #[must_use]
    pub fn from(&self) -> u16 {
        self.from
    }

    /// The version it produces.
    #[must_use]
    pub fn to(&self) -> u16 {
        self.to
    }

    /// Rewrites `world` in place, in the order the operations were declared.
    pub fn apply(&self, world: &mut World, schema: &Schema) {
        for step in &self.steps {
            match step {
                Step::RenameField { from, to } => {
                    if let Some(value) = world.remove(from) {
                        world.set(to.clone(), value);
                    }
                }
                Step::AddDefault { name, value } => {
                    if schema.declares_default(name) {
                        world.set(name.clone(), value.clone());
                    }
                }
                Step::Transform(rewrite) => rewrite(world),
            }
        }
    }
}

/// Every migration a build knows, ordered by version.
#[derive(Debug, Default, Clone)]
pub struct Migrator {
    steps: Vec<Migration>,
}

impl Migrator {
    /// Builds a chain, rejecting one that does not advance a version at a time.
    ///
    /// # Errors
    ///
    /// Fails on a duplicated `from` or a step that skips a version. Either leaves some save
    /// with no path to current, and finding that out at load time — with a player's file in
    /// hand — is worse than finding it out here.
    pub fn new(mut steps: Vec<Migration>) -> Result<Self, ChainError> {
        steps.sort_by_key(Migration::from);
        for pair in steps.windows(2) {
            if pair[0].from == pair[1].from {
                return Err(ChainError::Duplicate { from: pair[0].from });
            }
        }
        for step in &steps {
            if step.to != step.from.saturating_add(1) {
                return Err(ChainError::NotAdjacent {
                    from: step.from,
                    to: step.to,
                });
            }
        }
        Ok(Self { steps })
    }

    /// The versions a step starts from, ascending. For a test or a diagnostic.
    #[must_use]
    pub fn versions(&self) -> Vec<u16> {
        self.steps.iter().map(Migration::from).collect()
    }

    /// Applies migrations in order until `world` is at `to`.
    ///
    /// # Errors
    ///
    /// Fails with `E7201` naming the exact gap when no step starts from the version the
    /// world is at. The gap is *that step's* span, `version → version + 1`, not the whole
    /// distance to the engine: a chain with 5 → 6 but not 6 → 7 fails at 6, which is
    /// precisely the migration to write.
    pub fn migrate(
        &self,
        world: &mut World,
        schema: &Schema,
        from: u16,
        to: u16,
    ) -> Result<(), ReplayError> {
        let mut version = from;
        while version < to {
            let Some(step) = self.steps.iter().find(|step| step.from == version) else {
                return Err(ReplayError::MissingMigration {
                    from: version,
                    to: version.saturating_add(1),
                });
            };
            step.apply(world, schema);
            version = step.to;
        }
        Ok(())
    }
}
