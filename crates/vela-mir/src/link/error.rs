//! Why a program could not be linked.
//!
//! Each of these is a conflict between two modules that neither module could have seen on its own.
//! The MIR carries no spans — lowering drops them, and the bytecode is where a debugger needs them
//! — so a link failure names the modules involved instead of pointing at a line, which is what the
//! author needs to know anyway: which two files disagree.

use std::fmt;

/// Why a program could not be linked.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum LinkError {
    /// Two units claim the same module name, so their labels could not be told apart.
    DuplicateModule {
        /// The name both units claim.
        name: String,
    },
    /// Two modules declare the same `default`.
    ///
    /// World state is global and a `default` is a slot in the save schema, so this is one slot
    /// asked to hold two things rather than two slots. Renaming is not available: the save format
    /// and every script that reads it name the slot.
    DuplicateDefault {
        /// The name both declare.
        name: String,
        /// The two modules, in link order.
        modules: (String, String),
    },
    /// A label reference names a module the program does not have.
    ///
    /// `E2002` reports this at check time, so reaching it here means the reference and the module
    /// set disagree — a driver that linked a subset, most likely.
    UnknownModule {
        /// The qualifier as written.
        qualifier: String,
        /// The module the reference is written in.
        module: String,
    },
    /// Two modules declare the same effect with different signatures.
    ///
    /// An effect is a capability the *host* provides, by name: `rand.int` means one thing to the
    /// engine. Two modules disagreeing about its signature is a disagreement about what the host
    /// is being asked for, and picking one silently would run one module against the other's idea
    /// of the call.
    EffectMismatch {
        /// The effect's name.
        name: String,
        /// The two modules, in link order.
        modules: (String, String),
    },
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateModule { name } => {
                write!(f, "the module `{name}` was linked twice")
            }
            Self::DuplicateDefault { name, modules } => write!(
                f,
                "`{}` and `{}` both declare a `default {name}`; world state is global, so one \
                 name cannot be two slots",
                modules.0, modules.1
            ),
            Self::UnknownModule { qualifier, module } => write!(
                f,
                "`{module}` refers to a module `{qualifier}` that is not in the program"
            ),
            Self::EffectMismatch { name, modules } => write!(
                f,
                "`{}` and `{}` declare `{name}` with different signatures",
                modules.0, modules.1
            ),
        }
    }
}

impl std::error::Error for LinkError {}
