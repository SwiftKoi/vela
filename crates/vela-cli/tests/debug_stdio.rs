//! The debug adapter as a process, driven the way an editor drives it.
//!
//! The unit tests in `vela-debug` cover the stop policy and the checker against in-memory
//! fixtures. What they cannot cover is the half that only exists in a real run: that `vela debug`
//! starts, that a project is compiled by the *command line's* loader, that the protocol really
//! goes over the pipes, and that nothing else writes to the stream and corrupts it.
//!
//! So this spawns the binary and speaks the protocol over it — request, wait for the answer,
//! react — which is what a client does and therefore what a client's mistakes look like. It is
//! the same shape as `lsp_stdio.rs`, for the same reason.

use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};
use vela_debug::{read, write};

/// A project with two labels and a world variable the story changes.
fn project() -> PathBuf {
    let base = std::env::temp_dir().join(format!("vela-debug-stdio-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("src")).expect("create the fixture");

    std::fs::write(
        base.join("vela.toml"),
        "schema = 1\n\n[project]\nname = \"stdio\"\nentry = \"main.start\"\n",
    )
    .expect("write the manifest");
    std::fs::write(
        base.join("src").join("main.vela"),
        "default trust: int = 0\n\n\
         label start:\n    trust = trust + 5\n    \"First.\"\n    jump later\n\n\
         label later:\n    trust = trust + 1\n    \"Second.\"\n    return\n",
    )
    .expect("write the source");

    base
}

/// A client over a real adapter process.
struct Client {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    seq: i64,
    /// Events read while waiting for something else.
    events: Vec<Value>,
}

impl Client {
    /// Starts the adapter on a project.
    fn start(project: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_vela"))
            .arg("debug")
            .arg(project)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("start the adapter");

        Self {
            input: child.stdin.take().expect("stdin"),
            output: BufReader::new(child.stdout.take().expect("stdout")),
            child,
            seq: 0,
            events: Vec::new(),
        }
    }

    /// Sends a request and returns the response to it, keeping any events seen on the way.
    fn send(&mut self, command: &str, arguments: Value) -> Value {
        self.seq += 1;
        let seq = self.seq;
        let mut bytes = Vec::new();
        write(
            &mut bytes,
            &json!({ "seq": seq, "type": "request", "command": command, "arguments": arguments }),
        )
        .expect("frame");
        self.input.write_all(&bytes).expect("write");
        self.input.flush().expect("flush");

        loop {
            let message = read(&mut self.output)
                .expect("read a message")
                .expect("the adapter did not close the stream");
            if message["type"] == "response" && message["request_seq"] == seq {
                return message;
            }
            if message["type"] == "event" {
                self.events.push(message);
            }
        }
    }

    /// Sends a request that is expected to succeed.
    fn request(&mut self, command: &str, arguments: Value) -> Value {
        let response = self.send(command, arguments);
        assert!(
            response["success"].as_bool().unwrap_or(false),
            "`{command}` failed: {response}"
        );
        response
    }

    /// The next event with a name, reading more messages if it has not arrived.
    fn event(&mut self, name: &str) -> Value {
        if let Some(index) = self.events.iter().position(|event| event["event"] == name) {
            return self.events.remove(index);
        }

        loop {
            let message = read(&mut self.output)
                .expect("read a message")
                .expect("the adapter closed the stream before the event");
            if message["type"] == "event" && message["event"] == name {
                return message;
            }
            self.events.push(message);
        }
    }

    /// Stops the adapter, asserting it exited cleanly.
    fn finish(mut self) {
        drop(self.input);
        let status = self.child.wait().expect("the adapter finishes");
        assert!(status.success(), "the adapter exited badly");
    }
}

/// The `World` scope's reference, from a `scopes` response.
fn world_reference(scopes: &Value) -> i64 {
    scopes["body"]["scopes"]
        .as_array()
        .expect("scopes")
        .iter()
        .find(|scope| scope["name"] == "World")
        .expect("a World scope")["variablesReference"]
        .as_i64()
        .expect("a reference")
}

/// A variable's value, from a `variables` response.
fn variable(variables: &Value, name: &str) -> Option<String> {
    variables["body"]["variables"]
        .as_array()?
        .iter()
        .find(|variable| variable["name"] == name)
        .map(|variable| variable["value"].as_str().unwrap_or("").to_string())
}

/// A label breakpoint is hit, the stack and `World` are readable, and a step back rewinds the world.
#[test]
fn the_adapter_hits_a_breakpoint_steps_and_inspects_the_world() {
    let project = project();
    let mut client = Client::start(&project);

    let capabilities = client.request("initialize", json!({ "clientID": "test" }));
    assert_eq!(capabilities["body"]["supportsStepBack"], Value::from(true));
    client.event("initialized");

    let breakpoints = client.request(
        "setFunctionBreakpoints",
        json!({ "breakpoints": [{ "name": "later" }] }),
    );
    assert_eq!(
        breakpoints["body"]["breakpoints"][0]["verified"],
        Value::from(true),
        "the label is in the program"
    );

    client.request("launch", json!({ "stopOnEntry": true }));
    client.request("configurationDone", json!({}));
    assert_eq!(
        client.event("stopped")["body"]["reason"],
        Value::from("entry")
    );

    // Continue, and the label breakpoint stops it before `later` runs.
    client.request("continue", json!({ "threadId": 1 }));
    assert_eq!(
        client.event("stopped")["body"]["reason"],
        Value::from("breakpoint")
    );

    let stack = client.request("stackTrace", json!({ "threadId": 1 }));
    let frames = stack["body"]["stackFrames"].as_array().expect("frames");
    assert_eq!(frames[0]["name"], Value::from("main.later"));
    assert_eq!(frames[1]["name"], Value::from("main.start"));
    assert_eq!(frames[0]["source"]["name"], Value::from("main.vela"));

    // `World` shows what the story did on the way: `trust` was 5 before the jump.
    let world = world_reference(&client.request("scopes", json!({ "frameId": 0 })));
    let variables = client.request("variables", json!({ "variablesReference": world }));
    assert_eq!(variable(&variables, "trust").as_deref(), Some("5"));

    // One command back rewinds the world to before it was changed at all.
    client.request("stepBack", json!({ "threadId": 1 }));
    client.event("stopped");
    let world = world_reference(&client.request("scopes", json!({ "frameId": 0 })));
    let variables = client.request("variables", json!({ "variablesReference": world }));
    assert_eq!(
        variable(&variables, "trust").as_deref(),
        Some("0"),
        "stepping back rewinds the world, not just the line"
    );

    client.request("disconnect", json!({}));
    client.event("terminated");
    client.finish();

    let _ = std::fs::remove_dir_all(&project);
}

/// The recorded session in `docs/guides/debugger-walkthrough.md`, over the real example.
///
/// The guide shows a label breakpoint in `main.tally` stopping with a caller in another module, and
/// the values that go with it. Every line of that transcript is asserted here against
/// `examples/standard` and the real binary, so the document cannot describe a session the adapter
/// has stopped holding.
#[test]
fn the_walkthrough_over_the_standard_example() {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/standard");
    let mut client = Client::start(&project);

    client.request("initialize", json!({ "clientID": "walkthrough" }));
    client.event("initialized");

    let breakpoints = client.request(
        "setFunctionBreakpoints",
        json!({ "breakpoints": [{ "name": "tally" }] }),
    );
    assert_eq!(
        breakpoints["body"]["breakpoints"][0]["verified"],
        Value::from(true)
    );

    client.request("launch", json!({}));
    client.request("configurationDone", json!({}));
    assert_eq!(
        client.event("stopped")["body"]["reason"],
        Value::from("breakpoint"),
        "the story runs to `tally` without stopping on entry"
    );

    // The stack is three frames across two files: the label, the chapter that called it, and the
    // hub. That is what linking makes one program (`LANGUAGE.md §6.1`), seen from a debugger.
    let stack = client.request("stackTrace", json!({ "threadId": 1 }));
    let frames = stack["body"]["stackFrames"].as_array().expect("frames");
    let seen: Vec<(&str, &str, i64)> = frames
        .iter()
        .map(|frame| {
            (
                frame["name"].as_str().unwrap_or(""),
                frame["source"]["name"].as_str().unwrap_or(""),
                frame["line"].as_i64().unwrap_or(0),
            )
        })
        .collect();
    assert_eq!(
        seen,
        vec![
            ("main.tally", "main.vela", 105),
            ("chapters.street.arrive", "street.vela", 27),
            ("main.start", "main.vela", 99),
        ],
        "the caller is in a different module, and the line is the instruction about to run"
    );

    // `World` is where a visual novel's story lives, and it is readable while stopped.
    let world = world_reference(&client.request("scopes", json!({ "frameId": 0 })));
    let variables = client.request("variables", json!({ "variablesReference": world }));
    assert_eq!(variable(&variables, "trust").as_deref(), Some("0"));
    assert_eq!(variable(&variables, "nights").as_deref(), Some("0"));

    // And an expression is answered by the checker: a name by its value.
    let answer = client.request("evaluate", json!({ "expression": "trust", "frameId": 0 }));
    assert_eq!(answer["body"]["result"], Value::from("0"));
    assert_eq!(answer["body"]["type"], Value::from("int"));

    client.request("disconnect", json!({}));
    client.event("terminated");
    client.finish();
}

/// An expression the checker rejects is a diagnostic, not a crash.
#[test]
fn an_invalid_expression_is_a_diagnostic() {
    let project = project();
    let mut client = Client::start(&project);

    client.request("initialize", json!({}));
    client.event("initialized");
    client.request("launch", json!({ "stopOnEntry": true }));
    client.request("configurationDone", json!({}));
    client.event("stopped");

    // The failure is the response, so it is asked for without the success assertion.
    let rejected = client.send(
        "evaluate",
        json!({ "expression": "no_such_name", "frameId": 0 }),
    );
    assert_eq!(rejected["success"], Value::from(false));
    let message = rejected["message"].as_str().expect("a message");
    assert!(
        message.starts_with("E2"),
        "the checker's own code is the answer: {message}"
    );

    // A name that is in scope still answers with its value.
    let answer = client.request("evaluate", json!({ "expression": "trust", "frameId": 0 }));
    assert_eq!(answer["body"]["result"], Value::from("0"));
    assert_eq!(answer["body"]["type"], Value::from("int"));

    client.request("disconnect", json!({}));
    client.finish();

    let _ = std::fs::remove_dir_all(&project);
}
