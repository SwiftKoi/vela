//! `vela debug` — the debug adapter, on stdin and stdout or over a socket.
//!
//! The protocol owns the stream, so nothing else may write to it while this runs: a stray
//! `println!` is a corrupted message and a debugger that disconnects. Everything this command has
//! to say goes to stderr, or comes back as an `Error` the driver prints there.
//!
//! The project is compiled by [`compile_project`] — the same function `vela run` uses — and the
//! sources come back with it, which is what lets a breakpoint on a line find the instruction on
//! that line. That is the whole reason this is a command rather than a feature of `vela-debug`:
//! a project is a `vela.toml` and a set of files, and the command line is what knows how to read
//! those.

use std::io::{BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;

use vela_debug::{Program, Server};

use crate::command::{Command, Error};
use crate::commands::check::collect;
use crate::commands::run::{compile_project, flag_value, positional};

/// The `vela debug` command.
pub struct Debug {
    base: PathBuf,
}

impl Debug {
    /// Creates the command rooted at `base`.
    #[must_use]
    pub fn new(base: impl Into<PathBuf>) -> Self {
        Self { base: base.into() }
    }

    /// Creates the command rooted at the process's working directory.
    #[must_use]
    pub fn at_current_dir() -> Self {
        let base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self::new(base)
    }
}

impl Command for Debug {
    fn name(&self) -> &'static str {
        "debug"
    }

    fn about(&self) -> &'static str {
        "serve the debug adapter protocol, on stdio or a port"
    }

    fn run(&self, args: &[String], out: &mut dyn Write) -> Result<(), Error> {
        // Before anything is read: a project that does not compile must not make `--help` fail,
        // and the protocol has not started, so a message on stdout is still safe here.
        if args.iter().any(|arg| arg == "--help" || arg == "-h") {
            let _ = writeln!(
                out,
                "usage: vela debug [path] [--stdio] [--port <n>] [--start <label>]"
            );
            return Ok(());
        }

        let target =
            positional(args).map_or_else(|| self.base.clone(), |path| self.base.join(path));
        let project = collect(&target)?;
        let (module, entry, sources) = compile_project(&project, args, out)?;

        let mut server = Server::new(Program::new(module, entry, sources));

        match flag_value(args, "--port") {
            Some(port) => serve_on_port(&mut server, port, out),
            // `--stdio` is the default: it is what an editor launches, and it is the one that
            // needs no port to be free.
            None => {
                let mut input = std::io::stdin().lock();
                server
                    .serve(&mut input, out)
                    .map_err(|error| Error::internal(format!("the debug adapter stopped: {error}")))
            }
        }
    }
}

/// Waits for one client on a local port, then serves it.
fn serve_on_port(server: &mut Server, port: &str, out: &mut dyn Write) -> Result<(), Error> {
    let port: u16 = port
        .parse()
        .map_err(|_| Error::usage(format!("`{port}` is not a port")))?;
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|error| Error::internal(format!("cannot listen on port {port}: {error}")))?;

    // Printed to stdout, which is *not* the protocol stream here: a person launching this by hand
    // needs to see where to point the editor, and a message on the socket would be a protocol
    // message the client did not ask for.
    writeln!(out, "listening on 127.0.0.1:{port}")
        .map_err(|error| Error::internal(error.to_string()))?;
    out.flush()
        .map_err(|error| Error::internal(error.to_string()))?;

    let (stream, _) = listener
        .accept()
        .map_err(|error| Error::internal(format!("the client never connected: {error}")))?;
    let reader = stream
        .try_clone()
        .map_err(|error| Error::internal(error.to_string()))?;

    let mut input = BufReader::new(reader);
    let mut output = stream;
    server
        .serve(&mut input, &mut output)
        .map_err(|error| Error::internal(format!("the debug adapter stopped: {error}")))
}
