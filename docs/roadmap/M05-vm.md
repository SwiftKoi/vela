# M5 — VM and headless story runtime

**Goal.** A story runs to completion with no window, driven by mock capabilities.

**Depends on.** M4.

**Crates.** `vela-vm`, `vela-world`, plus the first mock host in `vela-vm/src/effects/`.

**Work items.**
1. `Vm`, `Frame`, step loop and `Step` outcomes exactly as `RUNTIME.md §1`.
2. `World` with `defaults`, `rng`, `clock`, `scene`, `audio`, `call_stack` (`RUNTIME.md §2`).
3. Core effects: `rand.*`, `time.now`, `input.choose`, `input.wait_click`; the capability
   table from `RUNTIME.md §3`.
4. Command construction (`Cmd`) and `Yield`/`resume` with **input-log recording from the
   first line of code** (`RUNTIME.md §1.3`). Retrofitting logging later invalidates every
   existing test.
5. Story constructs executed: say, menu, jump/call/return, if/match, pause, wait.
6. Replay equality test: record → replay → assert byte-identical `World` + command stream.
7. `vela run --headless` printing the command stream.

**Exit criteria.**
- [x] `examples/hello` runs headless to `Halt` and prints each say and menu command
- [x] Replay equality test green on a multi-branch fixture, **including a `rand` path** —
      replay is green, and so is the `rand` path: the generator lives in `World`, so an
      identical log draws identical numbers
- [x] The VM has **zero** dependencies on `vela-render`, `vela-ui`, or `vela-host` — asserted
      by `check-layers`' adapter rule, which `vela-vm` sits below
- [x] A deliberate `HashMap` use in the VM fails `check-determinism` — verified by adding one
      to `vela-vm/src/access.rs` and watching the gate name the file, the line, and the fix.
      `vela-vm` is not on the allowlist; the only entry is `vela-host`, which genuinely owns
      the platform clock
- [x] **Demo:** `vela run --headless --start main.start examples/hello`

**Status.** Complete. 254 tests, 7/7 `xtask` checks, clippy clean.

**The language could not call an effect, and now can.** `RUNTIME.md §3` lists `rand.*`,
`time.now`, and `input.*` with the capability table behind them, and M5 asks for the core
effects — but `LANGUAGE.md §3` had no `effect` declaration and no call form, so `rand.int(1,
6)` parsed as a field read on an undefined name. That was a hole between two specifications
rather than an implementation gap, and closing it took all six layers:

```vela
effect rand.int(low: int, high: int) -> int
effect time.now() -> float

label start:
    var roll = rand.int(1, 6)
```

`syntax` declares it, `hir` records it under its dotted name, `types` gives it a function
type, `mir` resolves a dotted call to it, `bytecode` carries the name and arity so a
separately built runtime knows what `rand.int` means, and the VM dispatches on that name
against `World`. Anything the host does not provide is `Fault::CapabilityDenied` rather than
an answer that is quietly wrong.

Two decisions the declaration forced, both found by trying to write one:

- **A segment may be a reserved word.** `audio.play` cannot parse unless `play` is accepted
  after a `.` — and `audio.play`, `audio.stop`, `input.wait` are precisely the capabilities
  that need the dotted scheme. After a dot there is no ambiguity to resolve.
- **An effect is a name holding a function.** It differs from a `fn` in who implements it,
  which is a runtime question. To a caller they are the same thing, which is why argument
  checking covers both — and why writing it exposed that **calls checked nothing at all**
  before this milestone.

**Found by running it.** Three serious bugs, all of which produced output that looked right:

- **The verifier and the machine disagreed about the command boundary.** The machine builds
  a command into a side register and the suspension hands the host's answer back in its
  place; the verifier modelled `Cmd` as pushing a command and `Yield` as replacing it. That
  reads better and describes a different program. They drifted by one slot, and it surfaced
  as a `dispatch` given `none`.
- **`Yield` falls through to the next instruction, and the resume block is not necessarily
  physically next.** A menu's blocks are opened before its arms, so their ids do not follow
  emission order — the arms fell through into each other. The verifier's rule 1 caught it.
- **A `Yield` with no result slot left the host's answer on the stack**, so a menu's arms
  reached the join one deeper than the dispatch's catch-all. Same rule, same detector.

**And a test helper that was too permissive.** The VM tests' `compile` asserted the result
*verifies* but never that the fixture *parses*. A stray double colon in a menu fixture made
the parser recover, silently drop the first choice, and produce a perfectly valid two-choice
menu — so a test about three-way branching was green while testing something else, and I
spent a while reasoning about dispatch tables that were correct. The helper now asserts the
front end was clean before compiling.

That is the same shape as the `render_demo` example from M1, which prints a hand-built
`E5003` complete with a `did you mean` suggestion the real analyzer does not produce. Both
are things that look like evidence and are not. A test that cannot fail for the reason you
think is worse than no test, because it is *reassuring*.

**Found later, by the first example that used them (M9).** The corpus exercised the machine
through shapes the *examples* never did — `examples/standard` had no defaults and every chapter
was one file — and two bugs lived in that gap. Both were silent: the story ran and printed
something plausible.

- **A function's parameters were not its frame.** `enter` set the frame's base to `stack.len()`,
  the *top* of the stack, while the caller had already pushed one value per parameter — so every
  parameter read found nothing. `local()` answering `none` for a slot that was not there is what
  made it silent: `n == 1` was false for every `n`, and the function quietly took the wrong
  branch. The base is now the bottom of the arguments, and a missing slot is `Fault::BadLocal` —
  a wrong base has to fail loudly, and `none` is exactly the value that made it look like a story
  bug. (`crates/vela-vm/tests/running.rs`.)
- **A `default` was neither seeded nor read back.** `default trust: int = 0` meant `none` until
  something wrote it, so the first line that touched it was `add.i` given `none`. A fresh run now
  seeds the world from the declarations (`RUNTIME.md §2`), and `LoadDefault` answers with the
  *declaration* for a name the world does not hold — which is the honest answer for a save written
  before that `default` existed, and for a world a caller built by hand. Restore deliberately does
  not seed: a loaded world holds what the save held.
