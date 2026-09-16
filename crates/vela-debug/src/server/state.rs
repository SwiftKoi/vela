//! The server: one debug adapter, and the story it is stepping.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};

use serde_json::{Value, json};

use crate::breakpoints::Breakpoints;
use crate::debuggee::Debuggee;
use crate::program::Program;
use crate::transport;

/// What a `variables` reference points at.
///
/// DAP hands out an opaque number for a container and asks for its contents later, so the number
/// has to mean something across two requests. These are the only containers this adapter offers:
/// a frame's slots, and `World`.
#[derive(Clone, Copy, Debug)]
pub(super) enum Scope {
    /// The named slots of a frame, by its position in the call stack (zero outermost).
    Locals(usize),
    /// `World`'s `default`s.
    World,
}

/// One debug adapter.
pub struct Server {
    /// The story under debug.
    pub(super) program: Program,
    /// The breakpoints set so far, which may arrive before the story starts.
    pub(super) breakpoints: Breakpoints,
    /// The running story, once `configurationDone` has started it.
    pub(super) debuggee: Option<Debuggee>,
    /// Whether the story is paused right now.
    pub(super) stopped: bool,
    /// Whether to stop before the first instruction rather than run to a breakpoint.
    pub(super) stop_on_entry: bool,
    /// The next message sequence number.
    pub(super) next_seq: i64,
    /// The scopes handed out by the last `scopes` request, cleared on every stop.
    pub(super) scopes: BTreeMap<i64, Scope>,
    /// The next `variables` reference to hand out.
    pub(super) next_reference: i64,
}

impl Server {
    /// A server for a compiled program.
    #[must_use]
    pub fn new(program: Program) -> Self {
        Self {
            program,
            breakpoints: Breakpoints::new(),
            debuggee: None,
            stopped: false,
            stop_on_entry: false,
            next_seq: 0,
            scopes: BTreeMap::new(),
            next_reference: 0,
        }
    }

    /// Answers messages until the client disconnects or the stream ends.
    ///
    /// # Errors
    ///
    /// Fails on a transport error, which is not recoverable: a stream whose framing is broken has
    /// no next message to resynchronise to.
    pub fn serve(
        &mut self,
        input: &mut impl BufRead,
        output: &mut dyn Write,
    ) -> std::io::Result<()> {
        while let Some(message) = transport::read(input)? {
            if !self.handle(&message, output)? {
                break;
            }
        }
        Ok(())
    }

    /// What this adapter can do.
    ///
    /// Only what is implemented, deliberately (`M11-debugger.md`, *Risks*): a capability an
    /// editor is told about is one it will call, and a call that answers nothing reads as a bug
    /// rather than as a gap. Everything absent here is refused explicitly in `dispatch`.
    pub(super) fn capabilities(&self) -> Value {
        json!({
            "supportsConfigurationDoneRequest": true,
            // A label breakpoint is `setFunctionBreakpoints`: the thing being named is a body,
            // which is what a label is.
            "supportsFunctionBreakpoints": true,
            // The rollback ring makes stepping backwards free (`RUNTIME.md §7`), so it is offered.
            "supportsStepBack": true,
            "supportsSetVariable": false,
            "supportsConditionalBreakpoints": false,
            "supportsHitConditionalBreakpoints": false,
            "supportsLogPoints": false,
            "supportsRestartRequest": false,
            "supportsTerminateRequest": false,
        })
    }
}
