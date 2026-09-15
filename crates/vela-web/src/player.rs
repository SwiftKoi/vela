//! The `Player` a web page drives.
//!
//! The engine boundary is `Command` values in and `Input` values out (`ARCHITECTURE.md §7`), so
//! this is a thin adapter over `vela_vm::Session`: it presents what the story yields and answers
//! it with what the page says.

use wasm_bindgen::prelude::*;

use vela_vm::{Session, Step};
use vela_world::Input;

/// A story being played from a web page.
#[wasm_bindgen]
pub struct Player {
    session: Session,
    finished: bool,
}

#[wasm_bindgen]
impl Player {
    /// Loads a compiled module and starts at `label`.
    ///
    /// The bytes are a `.velac` container — what `vela build` writes — rather than something this
    /// module knows how to produce. Compiling in the browser is possible and is a separate
    /// feature: the front end compiles for `wasm32` too, which is what makes an editor in a
    /// browser tab plausible at all.
    ///
    /// # Errors
    ///
    /// Fails if the bytes are not a module this engine understands, or the label is not in it.
    #[wasm_bindgen(constructor)]
    pub fn new(module: &[u8], label: &str) -> Result<Player, JsValue> {
        let module = vela_bytecode::decode(module).map_err(|error| {
            JsValue::from_str(&format!("the module could not be loaded: {error}"))
        })?;
        let session = Session::start(&module, label)
            .map_err(|fault| JsValue::from_str(&fault.to_string()))?;
        Ok(Self {
            session,
            finished: false,
        })
    }

    /// Runs to the next thing to present, and returns it.
    ///
    /// An empty string means the story ended. A string beginning `fault: ` is an engine bug, and
    /// it is *returned* rather than thrown because a page has to show it: a browser console is
    /// not where a player's problem should be discovered.
    pub fn step(&mut self) -> String {
        self.advance_with(None)
    }

    /// Answers the command on screen with nothing and runs on. Dialogue, mostly.
    pub fn line(&mut self) -> String {
        self.advance_with(Some(Input::Ack))
    }

    /// Answers the command on screen with a menu choice and runs on.
    pub fn choose(&mut self, index: usize) -> String {
        self.advance_with(Some(Input::Choice(index)))
    }

    /// Whether the story has ended.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn finished(&self) -> bool {
        self.finished
    }

    /// Runs on, answering first if there is an answer to give.
    fn advance_with(&mut self, answer: Option<Input>) -> String {
        if self.finished {
            return String::new();
        }

        let mut step = match answer {
            Some(input) => self.session.answer(input),
            None => self.session.advance(),
        };

        // A caller asked for *a thing to present*, and `Continue` means the engine has more work
        // to do before it needs the outside world — so it is not an answer to that question. The
        // VM's own loop cannot end on one today; this is here because `Step` says it can.
        loop {
            match step {
                Step::Continue => step = self.session.advance(),
                Step::Yield(command) => return command.to_string(),
                Step::Halt => {
                    self.finished = true;
                    return String::new();
                }
                Step::Fault(fault) => {
                    self.finished = true;
                    return format!("fault: {fault}");
                }
            }
        }
    }
}
