# Conventions

Rules for writing code in this workspace. These are requirements, not preferences. Where a
rule is mechanically enforced, the enforcing `xtask` check is named — see
`REPO_LAYOUT.md §4`.

## 1. The two rules that matter most

1. **Small files.** Hard budget in `REPO_LAYOUT.md §3`. A file that exceeds it is a bug,
   scoped like any other bug.
2. **Registries over `match`.** Any dispatch that grows with the number of kinds, widgets,
   formats, or effects must be a registry — a table populated at startup — not a `match`
   statement edited in place.

Rule 2 is the whole extensibility story. A `match` you must edit to extend is a closed
system; a registry is an open one. `xtask check-registries` fails CI when a dispatch `match`
appears outside the module that owns its registry.

**The carve-out: the front end.** The rule is about dispatch that *extension code* adds to —
effects, widgets, asset formats, command variants. The language's own grammar is closed: a
plugin cannot add a statement, an expression, or an item kind, so a `match` over node kinds
is the *right* structure there, because the compiler enforces that it is complete and a
registry would give that up. `check-registries` therefore exempts `vela-syntax`, `vela-hir`,
`vela-types`, `vela-mir`, and `vela-bytecode` as a class, and watches the runtime crates,
where a registry is the answer. The threshold elsewhere is deliberately coarse: this check is
a backstop for the convention, not the convention itself, and the judgement stays with review.

## 2. Rust rules

### 2.1 Errors
- Library crates **never panic** on user input. `unwrap`/`expect` are permitted only where a
  failure is a genuine bug (e.g. a table built by a build script), and must carry a message
  explaining the invariant.
- Fallible public APIs return `Result<T, E>` where `E` is a concrete per-crate error type;
  `anyhow` is allowed only in `vela-cli`, `xtask`, and tests.
- One error type per crate, defined in `error.rs`, implementing `std::error::Error`. Errors
  that reach the user go through `vela-diag`, never printed from deep inside a library.
- `thiserror` for defining errors; no hand-written `Display` where a derive will do.

### 2.2 Determinism (enforced)
Banned in **all** crates that affect program output:
- `std::collections::HashMap` / `HashSet` — use `IndexMap` / `IndexSet` (ordered) or
  `BTreeMap` / `BTreeSet`. Enforced by the source scan in `REPO_LAYOUT.md §4.2`.
- `std::time::SystemTime::now`, `Instant::now` outside `vela-host` — time is injected.
- `thread_rng`, `rand::random`, or any unseeded generator. RNG lives in `World`.
- `f64::to_string` / `format!("{}")` on floats where output is persisted or compared — use
  the pinned formatting helpers in `vela-world::fmt`.
- Address-based identity (`ptr as usize`) anywhere in serialized or compared state.
- Iterating a `rayon` result where collection order affects output.

`vela-host` is the single registered exception for clocks; it exposes a `Clock` trait, and
every other crate takes time as a value. `xtask check-determinism` scans source for these
patterns and fails with the offending path and line.

### 2.3 API surface
- Every public item has a doc comment. `#![warn(missing_docs)]` is on in every lib crate.
- Doc comments on public items state **what and why**, not how. Example:

```rust
/// Resolves every jump/call target in the module and reports unresolved ones.
///
/// Runs after parsing and before type checking: an unresolved label makes the
/// type checker's control-flow analysis meaningless, so we fail fast here.
pub fn resolve_labels(module: &mut Module) -> Result<(), ResolveError> { ... }
```

- Prefer `&str`/`&[T]` params, `impl Trait` for single-use generics, and concrete types in
  public signatures. Add a type alias before adding a fourth generic parameter.
- Sealed traits for registries and internal extension points; open traits only where third
  parties genuinely implement them (`Widget`, `Effect`, `Importer`, `Host`).

### 2.4 Style
- `cargo fmt` is canonical. No manual alignment.
- Imports: std, then external, then crate-local — one group each, sorted.
- `use` inside functions is discouraged except in tests.
- Prefer early return over nested `if`. Prefer `match` over `if/else if` chains on enums.
- Comments explain *why*, never *what*. A comment restating the code is deleted in review.

## 3. Testing

| Kind | Where | Purpose |
| --- | --- | --- |
| Unit | `crates/*/src/tests/` | One behavior per test; fast, no I/O |
| Integration | `tests/` at root | Cross-crate behavior (compile → run → assert) |
| Golden | `tests/golden/` | Diagnostics, bytecode disassembly, rendered frames |
| Story | `tests/stories/` | End-to-end through `vela-test` |

Rules:
- **Fix a bug by first adding the test that reproduces it.** The test lands in the same PR.
- **Golden tests show diffs.** `cargo xtask bless` regenerates them, and CI fails on any
  unblessed change so a diff is always human-reviewed, never silently accepted.
- Every diagnostic code in the registry has at least one test asserting the code *and* the
  primary span. `xtask check-diag-codes` fails if a code has no referencing test.
- Determinism tests run twice and compare byte-for-byte, on a seeded corpus.

## 4. The extension matrix

Each row is the **complete, ordered checklist** for adding a capability. If following the
list requires editing a file not named here, that is a bug in the architecture — fix the
registry, don't patch around it. This matrix is the contract that keeps
`ARCHITECTURE.md §5` honest.

### 4.1 Add a new statement (example: `pause 2.0 seconds`)

| # | File | Change |
| --- | --- | --- |
| 1 | `vela-syntax/src/lex/token.rs` | Add any new keyword to the keyword table |
| 2 | `vela-syntax/src/parse/stmt/pause.rs` | **New file.** Parse rule, < 100 lines |
| 3 | `vela-syntax/src/parse/stmt/mod.rs` | Register the rule in the statement table |
| 4 | `vela-syntax/src/types.rs` | Add the CST node variant |
| 5 | `vela-hir/src/stmt/pause.rs` | **New file.** Lowering to HIR |
| 6 | `vela-types/src/check/stmt.rs` | Type rule (scalar-typed argument) — registry entry, no new file if the family exists |
| 7 | `vela-mir/src/lower/stmt/pause.rs` | **New file.** Lower to MIR (emits a `Pause` effect) |
| 8 | `vela-vm/src/effects/pause.rs` | **New file.** Effect handler, registered in `EffectRegistry` |
| 9 | `tests/golden/parse/pause.vela` + `.expected` | Parse golden |
| 10 | `tests/golden/diag/E####_pause_bad_arg.*` | Diagnostic golden |
| 11 | `tests/stories/pause.vn` | Story test executing the statement |

Steps 3, 6, 8 are **registry insertions**, not core edits. Steps 2, 5, 7, 8 are new small
files. No file that already existed grows past budget.

### 4.2 Add a UI widget

| # | File | Change |
| --- | --- | --- |
| 1 | `vela-ui/src/widgets/<name>.rs` | **New file.** `measure`, `layout`, `paint`, prop schema |
| 2 | `vela-ui/src/widgets/mod.rs` | One `register!` line |
| 3 | `vela-ui/src/style/props.rs` | Add props to the shared prop table (if new props are needed) |
| 4 | `docs/spec/SCREENS.md` | Document props + an example |
| 5 | `tests/golden/ui/<name>.vela` + rendered frame | Layout golden |

### 4.3 Add an effect (host capability)

| # | File | Change |
| --- | --- | --- |
| 1 | `vela-vm/src/effects/<name>.rs` | **New file.** Signature + result type |
| 2 | `vela-vm/src/effects/registry.rs` | One table entry, declaring required capability |
| 3 | `vela-host/src/caps/<name>.rs` | Trait impl slot for native; mock in `vela-test` |
| 4 | `vela-test/src/mocks/<name>.rs` | **New file.** Deterministic mock |
| 5 | `tests/stories/<name>.vn` | Story test asserting the effect is invoked with expected args |

### 4.4 Add an asset importer

| # | File | Change |
| --- | --- | --- |
| 1 | `vela-assets/src/importers/<ext>.rs` | **New file.** Detect + decode → normalized asset |
| 2 | `vela-assets/src/importers/registry.rs` | One table entry: probe, extension, importer |
| 3 | `vela-assets/src/manifest.rs` | Only if a new asset kind needs a new manifest entry |
| 4 | `tests/golden/assets/<sample>` | Golden: input file → expected manifest digest |

Step 2 is a registry insertion, not an edit to a dispatcher: the selection rule — magic bytes
first, extension second (`BUILD_AND_ASSETS.md §3.1`) — lives once, in the registry, and an
importer declares what it claims rather than being named in an `if`. `importers/mod.rs` stays a
facade, which is why the row names `registry.rs` and not `mod.rs`.

### 4.5 Add a MIR optimization pass

MIR passes live with MIR (they are a layer-1 concern); `vela-compile` only orchestrates them.

| # | File | Change |
| --- | --- | --- |
| 1 | `vela-mir/src/opt/<name>.rs` | **New file.** Implements `Pass` |
| 2 | `vela-mir/src/opt/registry.rs` | One table entry; declare ordering vs. the passes in `BYTECODE.md §2.1` |
| 3 | `tests/golden/mir/<name>.vela` + `.expected` | Pretty-print golden showing the pass effect |
| 4 | differential test entry | The pass must preserve observable behavior (`BYTECODE.md §2.1`) |

Adding a **lint** is the same shape but lives in `vela-compile/src/lints/` with its registry
in `vela-compile/src/lints/mod.rs`; a lint additionally requires a diagnostic code and a test
referencing it (`check-diag-codes`).

### 4.6 Add a bytecode instruction (the deliberate exception)

| # | File | Change |
| --- | --- | --- |
| 1 | `vela-bytecode/src/op.rs` | One `OpSpec` table entry (name, operand kinds, stack effect) |
| 2 | `vela-bytecode/src/verify.rs` | Verifier rule for the new stack effect |
| 3 | `vela-vm/src/interp.rs` | One handler arm **or** a handler struct registered in the op-handler table |
| 4 | `vela-mir/src/lower/*` | Emit it (only if some construct needs it) |
| 5 | `tests/golden/bytecode/*` | Disassembly golden + a verifier rejection test |

Adding an instruction must not require touching `vela-vm`'s scheduling loop, only its
handler table.

### 4.7 Add a CLI subcommand

| # | File | Change |
| --- | --- | --- |
| 1 | `vela-cli/src/commands/<name>.rs` | **New file.** Implements the `Command` trait |
| 2 | `vela-cli/src/commands/registry.rs` | One entry: name, help, arg schema |
| 3 | integration test in `tests/cli/` | Assert exit code + stdout contract |

### 4.8 Add a platform backend

| # | File | Change |
| --- | --- | --- |
| 1 | `vela-host/src/backends/<name>/` | **New dir.** Implements `Host`, `Clock`, `Input`, `Fs` |
| 2 | `vela-host/src/backends/mod.rs` | Feature-gated registration |
| 3 | `xtask` adapter check | Confirm the backend does not leak into lower layers |

### 4.9 Add a save migration

A version step is a new file plus one line in the chain registry. `RUNTIME.md §6`.

| # | File | Change |
| --- | --- | --- |
| 1 | `vela-replay/src/migrations/v<NNNN>_<slug>.rs` | **New file.** One `migration!`, from one version to the next |
| 2 | `vela-replay/src/migrations/mod.rs` | Declare the module |
| 3 | `vela-replay/src/migrations/registry.rs` | One line: add it to the chain |
| 4 | `vela-replay/src/save.rs` | Bump `SAVE_VERSION` by one |
| 5 | `tests/golden/saves/fixture.vela` | Copy it to `fixture_v<N>.vela`, then update it to the new schema |
| 6 | `cargo xtask bless` | Seeds `v<N>_fixture.velasave` and rewrites the `.expected` goldens |

Steps 2 and 3 are registrations, and step 1 is a new small file — no existing file grows past
budget. Step 6 is not optional: the corpus is what runs the migration on every CI build, and a
version with no seeded save fails the corpus test by design.

## 5. Definition of Done

A change is done when **all** of these hold. This is the checklist a reviewer (human or
agent) applies, and the checklist `ROADMAP.md` milestones reference.

- [ ] Behavior implemented, with the extension-matrix checklist followed exactly
- [ ] Tests added: unit for the logic, golden for any diagnostic/bytecode/render change
- [ ] Every new diagnostic code has a test referencing it and is documented
- [ ] `cargo fmt`, `clippy -D warnings`, and all `xtask` checks pass
- [ ] No file exceeds the budget; no `match` moved dispatch into a core file
- [ ] Public items documented; crate `README.md` updated if the boundary changed
- [ ] Spec docs updated **in the same commit** when a documented contract changes
- [ ] If a plugin ABI or bytecode format changed, the version was bumped and `check-abi` passes

## 6. Commit and review

**Branching and merging.** `main` is always green and is never committed to directly. Work
happens on **one branch per implementation step** — `m1` for the whole first milestone, or
`<type>/<slug>` otherwise. A step is what lands as one squash commit; work items are commits
inside it, not branches. Name the branch for the step and leave the name alone: renaming
mid-flight severs the link to the commit it was cut from. Open the pull request when the work
is complete, not while it is being assembled. Every pull request
lands as a single squashed commit, so `main` reads as one commit per work item and intermediate
commits never need to be green. CI runs on a pull request targeting `main` and on a push to
`main`; pushing to a feature branch with no pull request does not run it. See
`docs/adr/0001-branching-and-ci.md`.

- Conventional commits: `feat(vela-vm): deterministic RNG advanced only by rand effect`.
- Scope is the crate. One logical change per commit; one milestone work item per PR.
- **Spec-first rule:** if implementing a feature reveals the spec was wrong, fix the spec
  first, then the code, in the same PR. The specs are the contract, and a contract that
  drifts from reality is worse than none.
- A PR may not raise the exemption count from `check-exemptions` without justification in the
  description.
