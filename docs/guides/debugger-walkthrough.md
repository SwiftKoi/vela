# Debugging a story

A walkthrough of the debug adapter, using `examples/standard` — the same three-module story the
language server's guide uses, so a breakpoint here lands in a program that crosses files. Step 1 is
asserted by `crates/vela-cli/tests/debug_stdio.rs`, against this project and the real `vela debug`
binary, so the document cannot drift away from what the adapter does. The later steps are asserted
by the tests named beside them.

## Pointing a client at it

The adapter speaks the Debug Adapter Protocol over stdin and stdout:

```
vela debug examples/standard
```

There is **no bundled editor extension yet** — that is deliberate, and the same state the language
server is in. Any DAP client can launch the command as its adapter. In Neovim:

```lua
local dap = require("dap")

dap.adapters.vela = function(callback)
  callback({ type = "executable", command = "vela", args = { "debug", vim.fn.getcwd() } })
end

dap.configurations.vela = {
  { type = "vela", request = "launch", name = "Debug the story", program = vim.fn.getcwd() },
}
```

`--start module.label` overrides the project's entry point, exactly as it does for `vela run`, so a
long story can be debugged from the label you are working on:

```
vela debug examples/standard --start main.back_from_street
```

`--port <n>` listens on `127.0.0.1` for one client instead of using stdin, which is what a client
that connects rather than spawns needs.

## 1. A breakpoint on a label

Set a **label breakpoint** on `tally` — the routine the chapters call and come back from — and
launch. The story starts at `main.start`, jumps into `chapters.street`, and stops before the first
instruction of `main.tally`.

Ask for the stack:

```
main.tally                 main.vela             :105
chapters.street.arrive     chapters/street.vela  :27
main.start                 main.vela             :99
```

Three frames across two files. That is the whole point of a debugger over a linked program: the
caller is in a *different module*, and nothing at run time knows that — the modules were linked
before codegen (`LANGUAGE.md §6.1`), so this is an ordinary call stack with ordinary names in it.

## 2. Looking at `World`

While stopped, ask for the scopes of a frame. There are two: **Locals**, the frame's own slots, and
**World**, the `default`s — which is where a visual novel's story actually lives.

```
trust      int     0
nights     int     0
```

`main.tally` has just been entered and nothing has changed `trust` yet. Set a breakpoint on
`main.ending` instead, let it run there, and the same view reads `trust 1` — the hub counted the
return in `back_from_street` on the way.

## 3. Stepping

Four ways to move, all expressed over the frame stack (`RUNTIME.md §9`):

| Request | What it does |
| --- | --- |
| `continue` | runs to the next breakpoint |
| `next` | the next statement in this frame, skipping calls |
| `stepIn` | the next statement, descending into a call |
| `stepOut` | runs until this frame returns |

`next` stops when a **statement** changes, not when an instruction does, so a line of dialogue with
three instructions behind it is one stop. The distinction is tested in
`crates/vela-debug/tests/stepping.rs`: stepping over a call never stops inside it, stepping into one
does, and stepping out leaves it.

## 4. Stepping backwards

Ask to step back and the story moves **backwards** one command — not by undoing anything, but by
restoring a snapshot the runtime was already keeping. `RUNTIME.md §7` makes rollback
snapshot-and-replay, so a debugger gets time travel for free: the ring holds a snapshot per command,
and going back is putting one back.

The world goes back with it. In the small fixture `debug_stdio.rs` uses, `trust` is `5` at a
breakpoint and `0` after one step back, because the snapshot is of the whole `World`, not of a line.
The stronger claim — that stepping back *ten* commands reaches byte-identical state to the forward
run — is
`crates/vela-debug/tests/stepping.rs`'s `stepping_back_reaches_the_state_the_forward_run_had`.

Two things the ring does not reach back past: a save that was loaded (rollback history is not
persisted, `RUNTIME.md §7.3`), and the oldest snapshot still in the ring.

## 5. Evaluating an expression

Hover or ask directly, and a **name** is answered with its value and its type:

```
trust  →  0          (int)
```

Anything else is checked with the *real* type checker, which is the promise in `TOOLING.md §6`: an
expression that does not compile gives a normal `Exxx`, never a crash inside an evaluator.

```
no_such_name  →  E2001: undefined name `no_such_name`
```

An expression that checks but is not a name — `trust + 1` — is answered with its type rather than a
value: `int (checked; only names are evaluated)`. Producing a value for it would mean compiling the
expression into the running story, and half an evaluator that sometimes invents a value is worse
than one that says what it knows. The checker and the scope rules are tested in
`crates/vela-debug/tests/evaluate.rs`.

## What is not here yet

Gaps rather than bugs, and each is *refused* rather than approximated — a request this adapter does
not implement comes back as a failure with a message, never as silence or a stop that does not
happen:

- **`reverseContinue`** — "run backwards to the previous breakpoint". The ring goes back a command
  at a time but does not record where breakpoints were, so this is refused and `step back` is the
  way back.
- **Conditional and hit-count breakpoints, log points, `setVariable`, restart.** Not implemented and
  not advertised in the capabilities, so a client will not offer them.
- **Menus take the first branch.** A debugged run has no player, so a `menu` is answered the way
  `vela run --headless` answers it. Choosing at a menu from the debugger is a feature for later.
- **No bundle debugging.** `vela debug` compiles a *project*; a built bundle carries no source map
  (`BYTECODE.md §5`), so point the debugger at the project rather than at `dist/`.
- **A `--release` build cannot be breakpointed by line.** The module carries no debug info, so
  `setBreakpoints` answers `verified: false` and says why. Label breakpoints and stepping by
  instruction still work.
- **An editor extension.** The adapter is standard DAP; a VS Code or Neovim plugin that registers
  it, and a `package.json` for the launch configuration, are not written yet.
