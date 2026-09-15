//! Unrecoverable VM failures.
//!
//! Almost every one of these is a compiler or engine bug, never something an author wrote —
//! which is why they are `E6xxx` in a diagnostic and why a report carries the instruction that
//! produced them. A verifier exists so that these cannot happen; this is what happens when it
//! was wrong.
//!
//! [`Fault::StaleFrame`] is the exception, and is deliberately still a fault. It is raised by
//! restoring a save into a story that changed underneath it: the machine cannot continue,
//! because the statement it was suspended at is not in the code any more. The alternative is
//! to resume at whichever instruction inherited the recorded index, which runs a *different*
//! story from the same save without saying so — and a silent wrong resume is the one outcome
//! worse than a refusal.

use std::fmt;

/// Something the VM could not do.
#[derive(Clone, PartialEq, Debug)]
pub enum Fault {
    /// An instruction needed more values than the stack had.
    EmptyStack {
        /// The instruction that wanted them.
        op: &'static str,
    },
    /// A local was read before anything wrote it.
    BadLocal(u32),
    /// A call named a function that is not in the module.
    BadFunction(u32),
    /// A call named a label that is not in the module.
    BadLabel(u32),
    /// A name was looked up that the module does not have.
    NoLabel(String),
    /// A label lives in another module, and modules are not linked yet (`E7104`).
    ForeignLabel(String),
    /// A jump landed outside the code.
    BadTarget(u32),
    /// A constant was not in the pool.
    BadConstant(u32),
    /// A string was not in the table.
    BadString(u32),
    /// An operand had a type the instruction cannot use.
    WrongType {
        /// The instruction.
        op: &'static str,
        /// What it needed.
        expected: &'static str,
        /// What it was given.
        found: &'static str,
    },
    /// Integer division or remainder by zero.
    DivisionByZero,
    /// `Unwrap` was reached with `none`, which the compiler proved could not happen.
    UnwrapNone,
    /// The story was already finished.
    Finished,
    /// A frame's suspension is not in the body it was restored into.
    ///
    /// The one fault a *save* causes rather than a compiler bug: the story was edited between
    /// the save and the load, so the statement the frame was waiting at is gone. See the
    /// module docs for why this is not resumed anyway.
    StaleFrame {
        /// The body, as `label:name` or `fn:name`.
        body: String,
    },
    /// An effect named a capability the host does not provide.
    CapabilityDenied(String),
    /// An effect index is not in the module's effect table.
    NoEffect(u32),
    /// A replay asked for an answer the recording does not have.
    LogExhausted(usize),
    /// A command's arguments did not fit its schema.
    BadSchema(String),
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyStack { op } => write!(f, "`{op}` needed more values than the stack had"),
            Self::BadLocal(slot) => write!(f, "local {slot} was read before it was written"),
            Self::BadFunction(index) => write!(f, "there is no function #{index}"),
            Self::BadLabel(index) => write!(f, "there is no label #{index}"),
            Self::NoLabel(name) => write!(f, "there is no label or function `{name}`"),
            Self::ForeignLabel(name) => {
                write!(f, "`{name}` is in another module, which is not linked yet")
            }
            Self::BadTarget(offset) => write!(f, "the jump target {offset} is not an instruction"),
            Self::BadConstant(index) => write!(f, "there is no constant #{index}"),
            Self::BadString(index) => write!(f, "there is no string #{index}"),
            Self::WrongType {
                op,
                expected,
                found,
            } => write!(f, "`{op}` needs {expected} and was given {found}"),
            Self::DivisionByZero => f.write_str("division by zero"),
            Self::UnwrapNone => f.write_str("`unwrap` was reached with `none`"),
            Self::Finished => f.write_str("the story has already finished"),
            Self::NoEffect(index) => write!(f, "there is no effect #{index}"),
            Self::LogExhausted(at) => write!(
                f,
                "the recording has no answer for suspension {at}, so this is not the run that was recorded"
            ),
            Self::CapabilityDenied(capability) => {
                write!(f, "the host does not provide the `{capability}` capability")
            }
            Self::StaleFrame { body } => write!(
                f,
                "this save is waiting at a statement `{body}` no longer has; the story changed \
                 since it was written"
            ),
            Self::BadSchema(name) => write!(f, "`{name}` was built with the wrong arguments"),
        }
    }
}
