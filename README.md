# Vela — a visual novel engine

A story is written in a script language that is **compiled**, not interpreted: names, types,
screens, and control flow are resolved before the game runs, so a mistake is a diagnostic in
an editor rather than a crash in front of a playtester.

## What it is built around

**A typed, analyzable script language.** The language stays close to what a visual novel
author already writes — dialogue is a bare string, a label is a heading, a choice is a list —
but it has a real front end behind it: a parser, a type checker, a story graph, and a language
server. A label that does not resolve is an error with a suggestion. An unreachable label is a
warning. The editor can say all of this while you type, because the whole program is available
to it at once rather than one statement at a time.

**Determinism as an invariant, not a feature.** Saves, rollback, and replay are consequences of
state being plain data plus a recorded input log, rather than machinery bolted on afterwards.
There is no hash iteration, no clock read the runtime did not inject, and no host call that is
not recorded — and build gates fail if any of those reappear. A recording of a playthrough
replays to the same outcome, byte for byte, which is what makes the headless test runner
possible at all.

**A headless-first runtime.** The engine's boundary is a stream of presentation commands and a
log of answers. Everything beyond that boundary — a window, a test, a replay, a save file, a
debugger — is a different consumer of the same two things. That is why a story runs in CI with
no display, why a screenshot is an artifact the engine *produces* rather than something scraped
off a screen, and why one code path backs both the window and the golden image.

**A GUI editor that edits the same files you hand-write.** Never a divergent project format.
The editor is a client of the same compiler the command line uses.

## The one rule that protects the rest

`"Hello, world."` must remain a one-line script. Every powerful feature in these specs is
*opt-in*. If a beginner cannot start with a single line of dialogue and grow into types,
screens, and plugins without rewriting, the engine has failed regardless of what else it does.

## Document map

| Doc | What it answers |
| --- | --- |
| [docs/VISION.md](docs/VISION.md) | Why Vela exists, principles, differentiators, non-goals |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Layers, crate map, data flow, extension points, key decisions |
| [docs/engineering/REPO_LAYOUT.md](docs/engineering/REPO_LAYOUT.md) | Workspace tree, per-crate layout, file-size budget, `xtask` linters |
| [docs/engineering/CONVENTIONS.md](docs/engineering/CONVENTIONS.md) | Code rules, the 500-line budget, and the **extension matrix** |
| [docs/spec/LANGUAGE.md](docs/spec/LANGUAGE.md) | Surface syntax, grammar, type system, diagnostics |
| [docs/spec/BYTECODE.md](docs/spec/BYTECODE.md) | MIR, bytecode format, instruction set, verifier |
| [docs/spec/RUNTIME.md](docs/spec/RUNTIME.md) | VM, World state, determinism, save/rollback/replay |
| [docs/spec/SCREENS.md](docs/spec/SCREENS.md) | Declarative UI, layout, styling, accessibility, hot reload |
| [docs/spec/TOOLING.md](docs/spec/TOOLING.md) | CLI, diagnostics, LSP, formatter, debugger, test runner |
| [docs/spec/BUILD_AND_ASSETS.md](docs/spec/BUILD_AND_ASSETS.md) | Asset pipeline, targets, web, delta patches, reproducibility |
| [docs/adr/](docs/adr/README.md) | Decision records for choices that are expensive to reverse |
| [ROADMAP.md](ROADMAP.md) | Milestones M0–M14, work items, exit criteria |

## Reading order

Start with **VISION** for the *why*, then **ARCHITECTURE** for the *shape*, then **ROADMAP**
for the *order of work*. The `docs/spec/*` files are references — read them when you implement
the corresponding milestone; they are written to be implemented against, not read cover to
cover.

## Status

- **M0** (bootstrap and guardrails) — complete: 22 crates scaffolded with ranks, the `xtask`
  architecture checks green, `vela new` and `vela --version` working.
- **M1** (front end) — complete: `vela-syntax` lexes and parses the language, `vela check`
  reports diagnostics, and a golden corpus pins how they render.
- **M2** (semantics) — complete: names, types, and the story graph are checked, behind an
  incremental query database that re-checks only what an edit touched.
- **M3** (MIR) — complete: checked programs lower to a typed CFG, five optimization passes run
  over it, and a reference interpreter proves they preserve behaviour.
- **M4** (bytecode) — complete: MIR compiles to a verified instruction stream, and modules
  round-trip through a versioned `.velac` container.
- **M5** (the VM) — complete: stories run headless, replay from a recording, and call host
  capabilities through declared effects.
- **M6** (presentation) — complete: text shaping and layout, a render graph on `wgpu`, a
  window, and a capture path that needs no display.
- **M7** (screens) — complete: a widget registry, a constraint layout solver, styles and
  themes, an accessibility tree, and hot-reload diffing. `vela run` evaluates a project's
  `dialogue` screen, lays it out, and paints it into the same draw list the renderer consumes,
  instead of drawing the dialogue box by hand — and opens further screens as a layered stack
  (`Escape` for a `pause` screen, arrows to move focus, `Enter` to activate a button's
  `action`).
- **M8** (save, rollback, replay) — complete: saves are versioned, checksummed, and written
  atomically, and a running story snapshots and restores. A rollback timeline replays from the
  nearest snapshot and rebranches by discarding the tail, so "step back" and "take the other
  branch" are one mechanism. A save from an older build is carried forward by a `migration!`
  chain rather than refused, and `tests/golden/saves/` keeps a real save for every version so
  an incompatible change fails CI instead of a player. In `vela run`, `Backspace` rolls back
  and a project's `pause` screen can call `quick_save()`/`quick_load()`; `cargo xtask budget`
  holds a 10k-`default` snapshot to `RUNTIME.md §7.2`'s 1 ms.
- **M9** (build, assets, web) — *in progress*: `vela build` compiles every module under `src/`
  to `.velac` and imports every file under `assets/` through a registry (magic bytes first,
  extension second) into a content-addressed `manifest.json`, both into one `dist/` bundle —
  and refuses to ship a story that does not check. `vela check` resolves `@"path"` against the
  manifest (`E7001`), so an asset renamed on one machine is a compile error rather than a blank
  rectangle. `--verify-reproducible` builds twice and compares, and runs as gate 6 in CI, and
  `--patch-from`/`--patch-out` with `vela patch apply` ship only what changed — a text-only
  change to a 600 KB bundle is a few hundred bytes, checked by a test. Transformers and font
  subsetting, the remaining importers, `W7001`, the target drivers, and drawing in a browser are
  still to come — though `crates/vela-web` already runs the engine in wasm and plays the same
  story the native build plays, at 287 KB and gated.

## Working in this repo

```sh
cargo build                     # build the workspace
cargo test --workspace          # unit tests
cargo xtask all                 # every architecture and policy check
cargo xtask bless               # regenerate golden files, then review the diff
cargo xtask --list              # what the checks do
cargo run --release -p xtask -- budget   # startup and frame time, against xtask/budgets.toml
cargo run -p vela-cli -- --version
cargo run -p vela-diag --example render_demo   # see a rendered diagnostic
```

Two harnesses are checked in, because later milestones reuse them:

```sh
tools/capture.sh --out frame.png examples/hello        # a frame as a PNG, with no display
tools/drive.sh   --keys Return,click1 examples/hello   # a window, driven by synthetic input
```

`tools/drive.sh` runs under `xvfb-run` and refuses outright if it sees your own display, so it
is safe to run while you are using the machine.

The CI gate order is in `docs/engineering/REPO_LAYOUT.md §6`; `cargo xtask` is wired as an
alias in `.cargo/config.toml`, so `cargo xtask check-layers` works from any subdirectory.

Two conventions worth knowing before editing anything: files have a **hard line budget**
enforced by `check-file-size` (500 for source, 120 for `lib.rs`/`mod.rs`), and any dispatch that
grows with the number of kinds must be a **registry** rather than a `match`
(`CONVENTIONS.md §4` has the per-capability checklist).
