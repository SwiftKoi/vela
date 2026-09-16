# M11 — Debugger and docs

**Goal.** Debug a story like software, including backwards.

**Depends on.** M10.

**Crates.** `vela-debug` (the DAP server), `vela-vm` (the step interface), `vela-cli` (`vela debug`).

## Work items
1. DAP server over the VM's trace hooks (`RUNTIME.md §9`).
2. Breakpoints by label and by line; step over/in/out across frames.
3. Variable view using debug slot names; `World` inspection.
4. Expression evaluation reusing the real type checker.
5. Time-travel stepping built on rollback snapshots.
6. Trace hooks compiled out of release builds.
7. Generated docs published and linked from the LSP hover text.

## Exit criteria
- [ ] VS Code attaches, hits a label breakpoint, steps, and inspects `World` — a DAP client does all
      four (`crates/vela-cli/tests/debug_stdio.rs` drives the real binary over stdio, and the
      walkthrough in `docs/guides/debugger-walkthrough.md` is asserted against `examples/standard`),
      but no editor extension ships yet, so the *VS Code* half is unverified
- [x] An invalid evaluate expression returns a normal `Exxx`, no crash —
      `debug_stdio.rs::an_invalid_expression_is_a_diagnostic` asserts `E2001`, and the checker path
      is `crates/vela-debug/tests/evaluate.rs`
- [x] Stepping backwards over 10 commands yields byte-identical state to the forward run —
      `crates/vela-debug/tests/stepping.rs::stepping_back_reaches_the_state_the_forward_run_had`
      compares the restored `World` against the one the forward run recorded
- [x] Release builds carry no debug info (`Header::flags` verified in a test) —
      `crates/vela-bytecode/src/tests.rs::a_release_module_says_it_has_no_debug_info`, which also
      round-trips the flag through the container, and
      `crates/vela-vm/tests/debugging.rs::a_module_without_debug_info_has_positions_but_no_spans`,
      which asserts the machine reports nothing to break on
- [x] **Demo:** recorded debugging session in the docs (`docs/guides/debugger-walkthrough.md`)

## Status

Complete but for the editor extension: `vela debug` serves the Debug Adapter Protocol over stdio or
a port, and `vela-debug` implements the whole list above — label and line breakpoints, stepping
forwards and backwards, the stack, the frame's slots and `World`, and expression evaluation through
the real checker. Work item 7 turned out to be mostly done already: the reference pages were
generated and pinned by `doc_golden.rs`, and what was missing was hover answering a widget or an
action from the same schema and linking to the page. Two decisions are worth keeping and are
recorded below; the rest of this file is what the milestone learned.

## Found during implementation

**The interface is pull-based, not hook-based.** `RUNTIME.md §9` asked for "trace hooks compiled
out of release builds", and the implementation found a better reading of the same requirement: the
machine executes one instruction when asked (`Vm::step`) and answers where it is (`Vm::site`) and
what its frames hold (`Vm::call_stack`, `Vm::locals`), and a debugger pauses by *not* stepping any
further. There is nothing to compile out, because a shipped game never calls any of it — the "no
runtime cost" the hooks were for is true by construction. What *is* gated is the flag: `Vm::site`
reports no span and `Vm::locals` no name when `Header::FLAG_DEBUG` is clear, from one place, so a
release module cannot be line-broken however hard a client tries. The spec was updated to describe
this in the same commit, because a contract that drifts from reality is worse than none.

**A dedicated `vela-debug` crate, at rank 9.** The other option — a DAP module inside `vela-lsp` —
would have reused the language server's framing, and was rejected because `vela-lsp`'s stated
boundary is *"a thin adapter over the query database, holds no state of its own"*, and a debugger
holds a running `Timeline`. Rank 9 puts it beside `vela-test` and `vela-lsp`: the crates that
assemble what the lower layers say. The cost is forty lines of `Content-Length` framing written
twice; the alternative was a shared framing crate for those forty lines, or a language server that
also runs stories.

**The stop policy is the debugger's.** Which site a breakpoint matches, what "step over" means
across a frame, and how far back a step goes are all comparisons against a `Site` — `vela-vm` knows
where it is and nothing about what that is for. That is what keeps the machine free of debugging
*and* keeps the policy testable on its own (`crates/vela-debug/tests/stepping.rs`). It also made
the time-travel criterion nearly free: `vela-replay`'s ring already snapshots per command, so
`stepBack` is a rollback and the ten-command claim holds for the reason `RUNTIME.md §7.1` gives.

**`evaluate` reuses the checker, and says what it does not know.** A name is answered with its
value, because that is what a variable view shows and what a hover asks about. Any other expression
is wrapped in a function appended to the paused file — the same trick `vela-test` uses for
`expect` — and put through the parser, `vela_hir::resolve_names`, and `vela_types::check`, so an
invalid expression is the compiler's own `Exxx` rather than an evaluator's crash. A valid one that
is not a name is answered with its *type*, labelled as such: producing a value would mean compiling
the expression into the running story and calling a function with arguments, which the machine
cannot do yet.

## Still open

- **An editor extension**, which is exit criterion 1's other half. The adapter is standard DAP, so
  this is a `package.json` and a launch configuration rather than protocol work — but until it
  exists, "VS Code attaches" is proven by a test client and not by VS Code.
- **`reverseContinue`**, refused rather than approximated: the ring goes back a command at a time
  but does not record where breakpoints were.
- **A menu in a debugged run takes the first branch**, because a debugger has no player to ask.
  Choosing at a menu from the client is a feature for later.
- **A built bundle cannot be debugged by line.** `vela debug` compiles a project; a bundle carries
  no source map (`BYTECODE.md §5`), and `vela doc --out docs/reference` is what puts a page where
  hover's link can find it.

## Risks

DAP is a large surface for limited scope. Mitigation: implement only the
capabilities listed; refuse others explicitly rather than half-implementing.
