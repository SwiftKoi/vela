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

### 2.5 Build profile
`Cargo.toml` turns off `debug` and `incremental` for the `dev` profile, and so for `test`, which
inherits it. Both are off for a reason that was measured rather than assumed.

Debug information is **78–85% of every linked artifact** — embedded in each `.rlib` and duplicated
into each of the ~100 test binaries — and nothing here is stepped through in a Rust debugger: a
failing test prints its own source location, and `vela debug` debugs *Vela* programs over the VM,
not the Rust engine. Without it a backtrace still names functions; it loses file:line and variables.
Incremental compilation caches only the workspace crates — never a dependency — and returns little
against the whole-graph `clippy --all-targets` and `test --workspace` this repository runs, while
leaving a second copy of each crate's debug-laden state on disk that cargo never reaps.

Neither is a rule against ever using them. Turn both back on **for one session**, never in a commit:

```sh
cargo build --config 'profile.dev.debug=2' --config 'profile.dev.incremental=true'
```

A profile that changes with whoever last wanted a backtrace is a profile nobody can reason about,
which is why the override is per-run and the decision lives here.

## 3. Testing

| Kind | Where | Purpose |
| --- | --- | --- |
| Unit | `crates/*/src/tests/` | One behavior per test; fast, no I/O |
| Integration | `crates/*/tests/` | Cross-crate behavior (compile → run → assert) |
| Golden | `tests/golden/` | Diagnostics and parses (`parse/diag_e####_*`), MIR, containers (`velac/`), assets, saves, analysis |
| Story | a `test` item in a `.vela` file beside the story | End-to-end through `vela test` (`TOOLING.md §5`) |

There is no `tests/stories/`: a story test lives in the language, beside the state it asserts about,
so the same file is a story and its own test suite.

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
| 1 | `vela-syntax/src/lex/token.rs` | The keyword, if the statement has one |
| 2 | `vela-syntax/src/parse/stmt/<family>.rs` | The parse rule, in the family it belongs to: `say.rs`, `flow.rs`, `scene.rs` |
| 3 | `vela-syntax/src/parse/stmt/dispatch.rs` | One line in the statement table |
| 4 | `vela-syntax/src/tree/stmt.rs` | The CST node variant |
| 5 | `vela-types/src/check/stmt.rs` | The type rule — one arm, no new file |
| 6 | `vela-mir/src/lower/stmt.rs` | Lower it to MIR — one arm |
| 7 | `tests/golden/parse/<name>.vela` + `.expected` | Parse golden |
| 8 | `tests/golden/parse/diag_e####_<name>.{vela,expected}` | Diagnostic golden, if it adds a code |
| 9 | a `test` item where the statement is used | A story test executing it (`TOOLING.md §5`) |

**There is no HIR step.** `vela-hir` resolves names and builds the story graph; it does not lower
statements, and `vela_mir::lower` reads the syntax tree plus the type environment directly. Steps 3
and 5–6 are insertions into a table or a match; step 2 is a new file only when the statement starts a
new *family*.

### 4.2 Add a UI widget

| # | File | Change |
| --- | --- | --- |
| 1 | `vela-ui/src/widgets/builtin.rs` | One `Widget` entry in `ALL`: name, category, prop schema |
| 2 | `vela-ui/src/widgets/schema.rs` | Only if it takes props no other widget has |
| 3 | `vela-ui/src/layout/containers.rs` | Only if it is a container the solver has to measure |
| 4 | `docs/spec/SCREENS.md` | Document the props and an example |
| 5 | `crates/vela-ui/tests/` | A layout test, and a golden frame if it draws |

Step 1 is the whole registration: `WidgetRegistry::builtin` reads `builtin::ALL`, so there is no
second list to keep in step. A widget's one-line description is `Widget::summary`, which `vela doc`
and hover both render (`TOOLING.md §9`).

### 4.3 Add an effect (host capability)

Effects are a **`match` in `crates/vela-vm/src/ops.rs`**, not a registry: three exist
(`rand.int`, `rand.float`, `time.now`), all of them answerable from `World`, and a table for three
rows would be indirection with nothing behind it. Anything the match does not name is
`Fault::CapabilityDenied`, which is the honest answer rather than a default.

| # | File | Change |
| --- | --- | --- |
| 1 | `crates/vela-vm/src/ops.rs` | One arm in `effect`, matching the name and its arguments |
| 2 | `vela-host/` and the `Host` trait | The real implementation, when the effect needs the outside world |
| 3 | a `test` item using the effect | A story test asserting it is invoked with the arguments it was given |

**This row is the matrix's known debt, stated rather than hidden.** `RUNTIME.md §3`'s capability
table lists `input`, `audio`, `fs`, and `debug`, and none of them is implemented: the host has a
window, input, and a clock, and the VM is handed injected state. The first effect that needs a host
is the point at which this becomes a registry in `vela-vm`, with the capability declared against each
entry — and this table is updated in the same commit.

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

**Lints are not a registry.** There is no `vela-compile/src/lints/`: a lint is produced by the phase
that owns the question — `W4002` and `W4003` by `vela-hir::reach`, the coverage and completeness
warnings by `vela-types`, `W4011` by the formatter — and reaches the user through the diagnostic
model like any other finding. A lint therefore needs a registered code and a test that references it
(`check-diag-codes`), and nothing else.

### 4.6 Add a bytecode instruction (the deliberate exception)

| # | File | Change |
| --- | --- | --- |
| 1 | `vela-bytecode/src/op.rs` | One `OpSpec` table entry (name, operand kinds, stack effect) |
| 2 | `vela-bytecode/src/verify.rs` | Verifier rule for the new stack effect |
| 3 | `vela-vm/src/exec.rs` or `control.rs` | One arm: `exec` for a data instruction, `control` for one that moves control |
| 4 | `vela-mir/src/lower/*` | Emit it, only if some construct needs it |
| 5 | `crates/vela-bytecode/tests/rejection.rs` | A module the verifier must reject for its rule |

Adding an instruction must not require touching `vela-vm`'s step loop, only its dispatch. The
instruction set is the one place a plugin cannot extend (`ARCHITECTURE.md §5`), which is why this row
naming files in three crates is the deliberate exception rather than a violation.

### 4.7 Add a CLI subcommand

| # | File | Change |
| --- | --- | --- |
| 1 | `vela-cli/src/commands/<name>.rs` | **New file.** Implements the `Command` trait |
| 2 | `vela-cli/src/commands/mod.rs` | Declare the module and re-export the type |
| 3 | `vela-cli/src/registry.rs` | One `register` line |
| 4 | `crates/vela-cli/src/tests/` | The exit code and the stdout contract |

Step 3 is the registration that `--help` reads, so a command cannot be missing from it. The
integration tests live in the crate rather than at the workspace root because they call `run_code`
directly; the ones that need a real process are under `crates/vela-cli/tests/`.

### 4.8 Add a platform backend

| # | File | Change |
| --- | --- | --- |
| 1 | `vela-host/src/<backend>.rs` | The backend: window, input, and a clock, behind the crate's traits |
| 2 | `vela-host/src/lib.rs` | Feature-gated selection of it |
| 3 | `xtask check-layers` | Confirm it does not leak into a lower layer |

`window.rs` is the only backend today, and it is one file rather than a directory because one
platform is one case. A second is a module beside it and a line in `lib.rs`; the layer rule is what
keeps either out of the compiler and the VM, and it is checked rather than trusted.

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

**One branch per implementation step.** A step is whatever lands as a single squash commit —
usually a whole milestone, sometimes something smaller. Work items are commits *inside* it, not
branches: splitting a step across branches buys nothing, because the merge is a squash either way.
`main` is always green and is never committed to directly.

| Name | Use |
| --- | --- |
| `m<N>` | A milestone step — `m1` for the whole front end. Traceable to `docs/roadmap/`. |
| `<type>/<slug>` | A step that is not a milestone: `chore/…`, `fix/…`, `docs/…`. |

**Name a branch for the step, and do not rename it.** A branch named after a sub-part goes stale the
moment the step grows past that part, and renaming mid-flight severs the link to the commit the
branch was cut from. Open the pull request **when the work is complete**, not while it is being
assembled.

**CI runs in exactly three cases** (`.github/workflows/ci.yml`):

- a push to `main` — verifies the commit that actually landed;
- a pull request **targeting `main`** — the merge gate;
- manual dispatch — test a branch without opening a pull request.

Pushing to a feature branch with no pull request does not start CI, so building a branch costs
nothing and the run happens once, on the final state. Superseded pull-request runs are cancelled
(`cancel-in-progress`, grouped by ref); a `main` run never is, so a broken merge cannot hide behind
a follow-up push.

**Every pull request lands as one squashed commit.** Intermediate commits never reach `main`, so
they need neither CI nor greenness — which is what makes the CI rule affordable — and `main` reads
as one commit per work item, which is the history to scan and to `git bisect` through. The
pull-request run tests the branch's contents; the post-merge run tests the *squashed* commit, which
is a new object, so the second run is not redundant.

*Rejected:* CI on every branch push (it would cover states that never merge, since pull requests are
opened when the work is done); merge commits (they clutter `main` with every in-progress commit);
rebase merge (the same, and it makes each intermediate commit something that must be individually
green — the opposite of the rule above); CI only on push to `main` (that makes `main` the first place
a change is tested, which inverts the purpose of architecture guards).

- Conventional commits: `feat(vela-vm): deterministic RNG advanced only by rand effect`.
- Scope is the crate. One logical change per commit; one milestone work item per PR.
- **Spec-first rule:** if implementing a feature reveals the spec was wrong, fix the spec
  first, then the code, in the same PR. The specs are the contract, and a contract that
  drifts from reality is worse than none.
- **Documents are updated the way `docs/README.md` says.** It is short, and it is the one place
  that says what a status note looks like, what a milestone file's sections are, and which pages
  are generated rather than written.
- A PR may not raise the exemption count from `check-exemptions` without justification in the
  description.
