//! What the host answers a suspension with.
//!
//! `RUNTIME.md §1.3`: a resume input is *always* captured in the log, and there is no path
//! by which the VM observes the outside world without that observation becoming part of
//! replayable state. This is that observation, in the one shape the machine handles.

use crate::value::Value;

/// An answer to a suspension.
#[derive(Clone, PartialEq, Debug)]
pub enum Input {
    /// Nothing to say: advance the dialogue, finish the wait.
    Ack,
    /// A menu entry, by its position in the offered list.
    Choice(usize),
    /// An effect's result.
    Value(Value),
}

impl Input {
    /// The value the machine puts where the command was.
    ///
    /// A menu's position becomes an `int`, because that is what the dispatch after it
    /// indexes by — so the *input log* and the *program's* view of that input meet in one
    /// conversion rather than two.
    #[must_use]
    pub fn resolve(&self) -> Value {
        match self {
            Self::Ack => Value::None,
            Self::Choice(index) => Value::Int(i64::try_from(*index).unwrap_or(0)),
            Self::Value(value) => value.clone(),
        }
    }
}
