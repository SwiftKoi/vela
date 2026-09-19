# Runtime Specification

Status: **draft, normative for M4–M8, extended by M11's debug interface (§9).**

The runtime is `vela-vm` (execution), `vela-world` (state), and `vela-replay` (snapshots,
rollback, saves, migrations). The design goal is single and absolute: **the same save, the
same inputs, and the same build produce the same outcome, bit for bit, on every platform and
across engine versions.**

Everything that looks like a separate subsystem — rollback, save/load, replay, testing — is a
consequence of that one property.

## 1. The VM

### 1.1 Model

A single-threaded, synchronous stack machine. No threads, no async runtime, no callbacks from
the host into script code.

```rust
pub struct Vm {
    frames: Vec<Frame>,
    stack: Vec<Value>,
    pending: Option<Command>,   // built by `Cmd`, consumed by `Yield`
    status: Status,
}

pub struct Frame {
    func: FuncId,
    ip: usize,
    base: usize,                // stack base for locals
    ret_block: Option<BlockId>, // where to resume after a label call
}
```

### 1.2 The step loop

The VM runs instructions until it must observe the outside world, then suspends. It is a
state machine with exactly four outcomes:

```rust
pub enum Step {
    Continue,          // executed, more work available
    Yield(Command),    // suspend: present this, resume on the host's ack
    Halt,              // story ended
    Fault(Fault),      // unrecoverable: always a compiler or engine bug (E6xxx)
}
```

```rust
loop {
    match vm.step(&mut world)? {
        Step::Continue   => continue,
        Step::Yield(cmd) => return Ok(Suspend::Command(cmd)),
        Step::Halt       => return Ok(Suspend::Done),
        Step::Fault(f)   => return Err(f),
    }
}
```

**The VM never calls the renderer.** It produces `Command` values and consumes `Event`
values. This is decision D7 in `ARCHITECTURE.md §7` and it is what makes headless testing,
replay, and save/restore the same mechanism.

### 1.3 Resuming

The host resumes with a result, which is recorded in the input log before being handed back:

```rust
let outcome = vm.resume(&mut world, Input::Ack)?;          // say advanced
let outcome = vm.resume(&mut world, Input::Choice(2))?;    // menu selected
let outcome = vm.resume(&mut world, Input::Value(v))?;     // effect returned
```

A resume input is *always* captured in the log. There is no path by which the VM observes
the outside world without that observation becoming part of replayable state.

## 2. World state

`World` is the complete mutable state of a running story. It is plain, typed data with no
host pointers, which is what makes it serializable and snapshottable.

```rust
pub struct World {
    pub defaults: DefaultStore,   // from `default` decls — the save schema
    pub entities: EntityStore,    // generational ids, never raw pointers
    pub rng: Rng,                 // the single deterministic generator
    pub clock: Tick,              // injected time, never read from the OS
    pub scene: SceneState,        // what is shown/hidden right now
    pub audio: AudioState,
    pub call_stack: Vec<FrameRef>,
    pub log_cursor: u64,          // position in the input log
}
```

**Design rules:**

1. **Schema is derived from source.** Every `default` declaration and every declared
   `struct`/`enum` contributes to the schema. Adding a `default` is a schema change and is
   handled by the migration engine (§6), not by "we hope the pickle still loads."
2. **Generational ids.** Entities are `(index, generation)` pairs. Stale references are
   detectable rather than silently aliasing a reused slot.
3. **No interior mutability.** No `RefCell`/`Cell` in `World`. Mutation is explicit and
   ordered, which is a precondition for reproducible replay.
4. **RNG is state, not a service.** `Rng` lives in `World` and is advanced only by the
   scripted `rand` operation. Two runs with equal logs have equal RNG sequences.

> **Implemented (M5, corrected in M9).** A fresh run **seeds the world from the module's
> `default` declarations** before the first instruction, so `default trust: int = 0` means zero
> and a snapshot taken at any point carries the state the story declared. A *restored* world is not
> seeded: it holds what the save held, and a name the save does not have reads as its declaration —
> which is also `LoadDefault`'s fallback, so a save written before a `default` existed still
> answers with the value that `default` declares rather than with `none`.
>
> That fallback is a correction: reading a `default` used to answer `none` for any slot the world
> had not been told about, which made the first `trust + 1` an `add.i` given `none`. The rule it
> restores is the one stated above — the world's defaults *are* the declarations.

### 2.1 The player's settings

`World` carries a second store beside the save schema: the **settings** a player chooses. Text speed,
auto-forward, skip behaviour, volumes, display mode — the vocabulary is the interface's
(`SCREENS.md §7`'s `preference` action and the screens over it), and what this section fixes is the
store's *lifetime*.

```rust
pub struct World {
    pub defaults: DefaultStore,   // the playthrough: in every save, undone by every rollback
    pub preferences: Preferences, // the player: in no save, undone by no rollback
    // …
}
```

**A setting is the player's, not the playthrough's**, and three consequences follow:

1. **A save never carries one.** The field is skipped by the serializer, so a save written by one
   player is the story and nothing else — a second player loading it keeps their own settings.
2. **A snapshot never carries one.** `World::snapshot` — what `Session::snapshot` copies, and what a
   rollback restores from — leaves the store empty, so a rollback undoes the story and not the
   player's choices.
3. **A resume is told whose settings it is for.** `Session::restore(module, snapshot, preferences)`
   takes them from the caller, because a resume that read them out of the snapshot would be reading
   them from the state the two rules above keep empty. A load passes the live player's; a rollback
   passes the ones of the session it is replacing.

> **Implemented (M12.2, in part).** `vela_world::Preferences` is an ordered name→value store — ordered
> because the settings file is written from a walk over it (`CONVENTIONS.md §2.2`) — `World::preferences`
> carries it, and `World::snapshot` is the story-only copy. The write path is `Session::preferences_mut`,
> which is where a settings screen ends, and `crates/vela-replay/tests/preferences.rs` asserts the three
> rules through the real types: a save carries none (in the decoded world *and* in the bytes), a snapshot
> carries none, a rollback keeps them, and a resume takes the caller's.
>
> **Where it lives is `vela-replay`'s settings file** (`settings.rs`, `settings.velaprefs` beside the
> saves): the same envelope as a save — magic, version, checksum, a readable payload — without the schema
> digest, because a preference has no declared *shape*: the vocabulary names them (`SCREENS.md §7.1`) and
> the store holds any name, so a file written by a newer build is read by an older one rather than
> refused. A rename in the store is a version bump with a step in that chain, and a gap is `E7201` naming
> the exact step — which is what a player sees instead of preferences quietly resetting to the defaults.
> `tests/golden/settings/` keeps a real file for every version, seeded the way the save corpus is (§6.3),
> and `vela run` reads the file at startup: a start is the one moment those settings exist in
> `Preferences` and nowhere else.
>
> **Not yet.** The two halves that make a setting *do* something: nothing writes one (the `preference`
> action is dispatched by nobody, so there is nothing for the file to be written *from*) and nothing reads
> one (no screen draws a setting, and the transport that would obey a text speed is item 4's). The
> *vocabulary* and the *file* are in; every registry entry says `read: false` until one of those lands.

## 3. Capabilities / host interface

Scripts reach the outside world only through declared effects. Each declares the capability
it needs, and the host implementation is injected.

| Capability | Effects | Native impl | Test impl | Notes |
| --- | --- | --- | --- | --- |
| `input` | `input.choose`, `input.wait_click` | `vela-host::Input` | scripted from the test file | Recorded in log |
| `audio` | `audio.play`, `audio.stop`, `audio.position` | `vela-host::Audio` | mock returning fixed positions | Position is recorded |
| `fs` | `fs.read_save`, `fs.write_save` | `vela-host::SaveDir` | in-memory | Path confined to save dir |
| `rand` | `rand.float`, `rand.int`, `rand.pick` | `World::rng` (no host) | same | Fully deterministic |
| `time` | `now`, `sleep` | `Host::Clock` | virtual clock advanced by tests | Recorded |
| `debug` | `log`, `assert` | stderr / DAP | captured | Dev-only; stripped in release |

A capability a plugin did not declare is not callable — the registry returns
`Fault::CapabilityDenied`, and the plugin host never links the symbol. This is the mechanism
behind `ARCHITECTURE.md §6.3` and §6.4.

> **Not yet.** Three effects exist and no more — `rand.int`, `rand.float` and `time.now`, all answered
> from `World` with no host at all. The `Native impl` column names the owner each *other* effect would
> have rather than a type that exists: there is no `vela-host::Input`, `vela-host::Audio`,
> `vela-host::SaveDir` or `Host::Clock`, and `Audio` is a type in `vela-world` rather than a host
> capability. `CONVENTIONS.md §4.3` records the same debt as the extension matrix's known one.

## 4. Determinism

### 4.1 The contract

> Given an identical build and an identical input log, the runtime must reach an identical
> `World` and produce an identical command stream.

### 4.2 Sources of nondeterminism and their treatment

| Source | Treatment | Enforced by |
| --- | --- | --- |
| Wall clock | `World::clock: Tick`, read by the `time.now` effect | `check-determinism`, no `Instant::now` outside `vela-host` |
| `HashMap` order | Banned in output-affecting crates; `IndexMap`/`BTreeMap` only | `clippy::disallowed_types` |
| RNG | Single `World::rng`, never reseeded outside New Game | Test: replay equality |
| Float formatting/arithmetic | Pinned formatter; no fast-math; no FMA contraction | Platform test matrix: x86_64, aarch64, wasm |
| Thread scheduling | VM is single-threaded; parallelism only in asset build | `check-layers` (VM has no thread deps) |
| Address identity | Generational ids only; no `ptr as usize` in state | `check-determinism` |
| Host side effects | Every result recorded in the input log | `resume` API shape |
| Object/iteration tie-breaks | Explicit stable ordering everywhere | Review + tests |

### 4.3 How it is verified

- **Replay equality test** (`vela-test`): run a story corpus, record the input log, replay,
  assert `World` and command stream are byte-identical. Runs on every CI build.
- **Cross-platform matrix**: the same test on `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`,
  and `wasm32-unknown-unknown`. Float mismatches surface here, not in a player's save.
- **Fuzz**: random input logs replayed twice (must match) and against re-optimized bytecode
  (must match). This is the differential test from `BYTECODE.md §2.1` extended to the
  runtime.

## 5. Save format

A save is a serialized `World` plus metadata. It is versioned, readable, and migratable.

```rust
pub struct Save {
    pub header: SaveHeader,
    pub world: WorldBlob,
    pub frames: Vec<FrameBlob>,     // VM call stack
    pub log_len: u64,               // input log length at save time
    pub checksum: u64,
}

pub struct SaveHeader {
    pub magic: [u8; 4],             // b"VSAV"
    pub save_version: u16,
    pub schema_digest: [u8; 32],
    pub engine_build: BuildId,
    pub created_at: u64,            // display only; never read by game logic
    pub slot: SlotMeta,             // chapter name, playtime, thumbnail ref
}
```

**Properties that matter:**

- **Self-describing.** The blob carries the schema digest, so a mismatch is detected at load
  and routed to the migration engine rather than producing garbage.
- **No pickling of code.** Only data is saved. A save can never smuggle in executable state,
  and loading an old save cannot fail because a class changed shape.
- **Separate sidecar for the thumbnail.** Keeps the save small and fast; the thumbnail is
  regenerated if missing.
- **Checksum over payload**, so a truncated write is detected and the previous save is used.
- **Atomic writes**: write to a temp file in the save dir, fsync, rename. A crash mid-save
  never destroys the previous slot.

> **Implemented (M8).** The container, the schema digest, and the version discipline
> exist (`vela-replay`). The file is `magic | version | schema_digest | payload | checksum`, and
> the payload is JSON — the envelope is binary so a bad file is rejected before it is parsed,
> and the payload is text so a save can be read and diffed by hand. The checksum covers the
> bytes before it, which is why a **truncated file is caught by the checksum** rather than by a
> length field the truncation would have removed. Writes are temp-file-and-rename.
>
> A save's `save_version` decides the path: newer than the engine is `E7202`, older is `E7201`
> naming the gap (the migration chain is the next step, so *every* older save currently reports
> one), and a matching version with a different `schema_digest` is `E7204`. The digest is
> deliberately non-cryptographic (`vela-replay::digest`): it detects a changed schema, not a
> forged file.
>
> A save carries a [`vela_vm::Snapshot`] — the `World`, the machine's frames and operand stack,
> and the command that was on screen. **Frames are written by name** (`label:start`), not by
> index into the module: an index is not stable across a recompile, and a save written before
> an edit would otherwise resume in whatever function landed at that number. `Session::restore`
> puts the machine back and re-presents the command, and the input log starts empty (§7.3).
>
> A load goes through `Save::load` rather than the strict decoder: a save at the current version
> is checked against the build's schema, and an older one is carried forward by the migration
> chain (§6) first. What a caller holds is always current — the returned save carries the
> current version and schema digest — so a partly-migrated world never escapes the loader.
>
> **Note.** A suspension is anchored, not indexed. Naming the body is only half of surviving a
> rebuild: the machine is also suspended at a *statement*, and an instruction index is a
> position in one particular compilation. The same source built at a different optimization
> level lays its instructions out differently, so an index-only restore resumes in the middle
> of whatever moved into place. A frame therefore records the source range of the statement it
> is waiting at (`vela_vm::Resume`), keeps the index beside it as a hint, and **fails with
> `Fault::StaleFrame` when the statement is not in the body any more** rather than resuming
> somewhere plausible — a save whose story was edited is refused, not run wrong. Saves written
> before the anchor existed carry no statement and fall back to the index.
>
> That field is save version 3. A version step is usually a *world* rewrite; this one rewrites
> nothing, because what changed was the shape of the file rather than the state in it (§6.2).
>
> **Not yet.** Still open, and a syntax decision: an anchor that survives an *edit*. The anchor above
> survives a *rebuild* — a recompile, an optimization level, a different layout — but it is a
> position in a file, and a position moves when the file does: insert a line above a `call` and
> every save suspended inside that call is refused as stale. Ren'Py's answer is worth copying: a
> `from` clause on the call names the return site, and its build *inserts* the clauses it finds
> missing. That is also what a translation key wants (a message id that outlives the words around
> it) and what a warped-to statement wants, so one mechanism would serve saves, translation, and
> `M11`'s warp. What is settled here is only that the anchor must become *nameable*; the syntax
> lands with the first of those three, and before a patch is shipped rather than after.

## 6. Save migrations

The single feature that most distinguishes Vela from a bolted-on save system: **a save from
build N loads in build N+3 given only the added migration steps.**

### 6.1 Declarative migrations

```rust
// crates/vela-replay/src/migrations/0007_trust_to_affection.rs
migration! {
    from = 6,
    to = 7,
    rename_field = ("trust", "affection"),
    add_default = ("affection_cap", Value::Int(10)),
    transform = |w| w.set("affection", w.get("affection") * 2),
}
```

### 6.2 The chain

1. Load the save, read `save_version`.
2. If it equals the current version, deserialize directly.
3. If it is older, apply migrations in order until current.
4. If a migration is **missing**, fail loudly with `E7201` naming the exact version gap — never
   silently load a partially-migrated world.
5. If the save is *newer* than the engine, fail with a clear "this save is from a newer
   version" message (`E7202`).

> **Implemented (M8).** `vela-replay::migrations` holds the chain: `Migration` — the
> three operations above — `Migrator` to order and apply it, and the `migration!` macro a step
> is written with. One version step is one file, and `migrations::registry` lists them; adding
> a step is a new file plus one line there, never an edit to a growing `match`.
>
> `Save::load` reads the `save_version`, applies steps in order until current, and fails with
> `E7201` naming the *step* it could not take — a chain with `4 → 5` but not `5 → 6` reports
> `5 → 6`, not the whole distance — rather than loading a partly-migrated world. `Migrator::new`
> rejects a chain that is not one version at a time when it is built, so that failure is a bug
> in the table rather than a surprise with a player's file in hand.
>
> **Note.** A step may rewrite nothing. Version 3 changed the shape of the *file* — a frame gained the
> statement it is suspended at (§5) — and left the world exactly as it was, so its migration has
> no operations at all. It is still a step, and that is the point: `save_version` is a claim
> about the file, and "this build accepts a version-2 save" is spelled as a step in the chain
> rather than as a special case in the loader.
>
> Two points the implementation made concrete:
>
> * `transform` takes a `fn(&mut World)`, not a closure with captures. A migration is fixed:
>   one that could capture its surroundings could migrate the same save differently on a
>   second run, and the corpus would not catch it.
> * `add_default` writes only when the *current* schema declares that default. The chain is
>   shared by every project, so an unconditional write would put a field into the worlds of
>   projects whose stories never declared one — and it would then ride along in their saves.

### 6.3 Testing migrations

Migrations are the highest-risk code in the engine: they run on players' irreplaceable saves.
Therefore:

- `tests/golden/saves/` keeps a real save file for **every** historical version, forever.
- CI loads every one of them into the current build and asserts the expected `World`.
- The test corpus is append-only: deleting a historical save file requires an explicit
  maintainer decision recorded in the PR.

> **Implemented (M8).** `tests/golden/saves/` holds a save for every version, and
> `crates/vela-replay/tests/corpus.rs` loads each into the current build and asserts the world
> it produces against a `.expected` golden. The assertion is over `1..=SAVE_VERSION`, so a
> version bump with no seeded save — or a deleted historical file — fails CI rather than a
> player: that is what makes "append-only" a property the test can see.
>
> `cargo xtask bless` seeds the save for the *current* version when it is missing, by compiling
> the fixture, running it to a suspension, and snapshotting it — a real save, not a fabricated
> one — and rewrites the `.expected` files. It never rewrites a historical save.

## 7. Rollback and replay

### 7.1 Model

Rollback is snapshot-and-replay, not undo-log inversion. Snapshotting a plain-data `World` is
cheap and obviously correct; inversion has to be right for every operation and inevitably
isn't.

```
New Game ──► Snapshot S0
   │  commands…  every K commands take a snapshot
   ▼
S_i ──replay input log[i..j]──► exact state at command j
```

Changing a choice means: rewind to the most recent snapshot ≤ that command, replay the log
up to it, discard the tail, and continue with the new input. The discarded tail is where
"rewind and take the other branch" comes from — it is the *same mechanism* as rollback.

### 7.2 Budget

- Default snapshot interval: every 64 commands (tunable per project).
- A snapshot stores a full `World` plus a compressed frame stack. The cost budget is defined
  as *snapshot must not exceed 1 ms for a project with 10k `default` values*, enforced by a
  benchmark in CI. If a project exceeds the budget, the interval auto-tunes.
- Frames are snapshotted by value; because there are no host pointers, this is a memcpy of
  plain data, not a serialization round-trip.

> **Implemented (M8).** `cargo xtask budget` measures it: it builds a world with 10k
> `default` values beside a short call stack, takes the median of 100 snapshots, and holds it
> to the 1 ms above — which, unlike the startup and frame budgets, is the spec's own number
> rather than one derived from a measurement. The harness refuses to run in a debug build, so
> the number it prints is the engine's and not the absence of an optimizer's. Measured 0.37 ms
> on the development machine.
>
> **Not yet.** The auto-tuning half — a project whose snapshots exceed the budget still takes
> one every 64 commands, so it pays the cost rather than spacing snapshots further apart.

### 7.3 Interaction with saves

Rollback history is **not** persisted. A save records `log_len` and the current `World` only —
and not the player's settings, which no save and no snapshot carries at all (§2.1). On load, the
rollback buffer starts empty and refills as the player continues. This keeps saves small and makes
them independent of session length.

> **Implemented (M8).** `vela-replay::Timeline` wraps a session with a snapshot ring —
> interval 64 commands, depth 32 snapshots by default — and replays from the nearest snapshot
> at or before the target. Frames are copied by value (`VmState`), not round-tripped through a
> format, as §7.2 requires.
>
> The §7 exit criterion holds and is asserted on `World` bytes, not on a field: rolling back to
> each of 20 points reproduces the same serialized world *and* the same command. Rewind-and-
> rebranch is the same call — a rollback discards the tail, and the next answer is a new one —
> and rolls back to the other branch at a menu.
>
> **Implemented (M8).** `vela run` binds `rollback` (Backspace) to a step back, and a project's
> `pause` screen can call the `quick_save()`/`quick_load()` actions; saves live in a `saves/`
> directory beside the project. `examples/standard`'s pause menu has Save and Load buttons. A
> load goes through the migration chain, so a save from an older build is carried forward and
> the console reports `load <slot> (migrated N -> M)`.
>
> **Not yet.** A rollback or load re-applies the command now on screen but does not rebuild the
> staged scene from the restored world, so a rollback across a `scene` change leaves the old
> backdrop until the next one.

## 8. Headless mode

`vela-test` drives the identical VM with mock capabilities and no window, renderer, or audio
device. Because the VM has no dependency on presentation (`check-layers` guarantees it), the
test runner is the *same engine*, not a reimplementation:

```rust
let mut session = Session::load("tests/stories/forest.velac")?;
session.mock_input(["menu:1", "click", "click"]);
session.mock_rand(seed = 42);
while let Some(cmd) = session.next_command()? {
    session.assert_command_matches(cmd, expectations.current())?;
}
session.assert_world(|w| w.get_int("trust") == 1);
```

This is what makes stories testable in CI — the differentiating feature from `VISION.md §3.1`.

> **Implemented (M9).** `vela_vm::Session::load` exists and does what the example asks:
> given a built bundle directory it reads the entry point from the bundle's manifest and the
> module from `scripts/`, and starts the story — no source is read and no compiler is involved,
> which is `ARCHITECTURE.md §8`'s *"bytecode loads without recompilation"* made true. Given a
> lone `.velac` it starts at the module's first label, so `Session::load("story.velac")` works
> when there is no manifest to read.
>
> The loader is **native-only**: a browser has no bundle directory, and a page constructs
> `vela_web::Player` from bytes it fetched (`BUILD_AND_ASSETS.md §5`). Gating it off `wasm32`
> also keeps the JSON reader it needs for the manifest out of the size-gated wasm module.
>
> `vela run <bundle>` is the same loader behind the CLI, and `crates/vela-cli/src/tests/`
> observes that it plays the same command stream as the story does from source — after the
> source tree has been deleted, so nothing *could* recompile.
>
> **Note.** What it starts at is the whole entry point. A bundle holds a *linked* program, so
> `main.start` is a label's name rather than a module and a label: the manifest's `entry` is
> passed to the machine as written, and the image is found by reading that name as a path
> (`main.start` → `scripts/main.velac`). A story split across files therefore runs from a bundle
> with nothing to link at load: the linking happened at build time (`LANGUAGE.md §6.1`).
>
> The `mock_input` / `assert_world` half of this example is `vela-test`'s, and is not written
> yet: the session API it drives is here, the assertion DSL around it is the next step.

## 9. Debugging

The VM exposes a **step-level interface** consumed by the DAP server (`TOOLING.md §6`). It is
*pull-based*: the machine executes exactly one instruction when asked (`Vm::step`) and answers
questions about where it is (`Vm::site`) and what its frames hold (`Vm::call_stack`,
`Vm::locals`). A debugger pauses by simply not stepping any further. There is no hook the machine
calls out to, so a build that never debugs pays nothing — the "no runtime cost in shipped games"
this section asks for, made true by construction rather than by a feature flag.

- **Breakpoint on `(module, label, line)`.** A label breakpoint is a *site* whose body is that
  label; a line breakpoint is a site whose span covers that line. Both are a comparison the
  *debugger* makes, not a facility inside the machine.
- **Step over / into / out**, expressed over the frame stack: `Site::depth` and
  `FrameInfo::depth` are the depth each frame sits at, and a `Yield(Command)` is a stop like any
  other.
- **Inspect** `World.defaults` (through the session's `World`), locals (named via debug slot
  names, `BYTECODE.md §5`), and the call stack.
- **Evaluate** an expression in the paused frame's scope — this reuses the type checker, so an
  invalid expression gives a normal `Exxx` diagnostic instead of an interpreter crash.

**Compiled out of release builds.** A build without debug info clears `Header::FLAG_DEBUG`
(`BYTECODE.md §5`), and the machine then reports neither a span nor a slot name — `Site::span` is
`None` and `DebugLocal::name` is empty — from the single place the flag is read. A line
breakpoint therefore has nothing to match on a released module, and the debugger refuses it
rather than inventing a line.

## 10. Performance posture

Predictability over peak throughput, restated for the runtime:

- **Startup**: load bytecode, deserialize the schema, validate a save — all O(size), no
  recompilation, no scripting warmup.
- **Per command**: bounded — one command is a handful of instructions plus one snapshot amortized
  over `K` commands.
- **Rollback latency**: replaying a bounded number of commands (interval × snapshot cost).
  Target: imperceptible for a 64-command interval on the benchmark corpus.
- Everything else is measured before it is optimized. The VM is not the bottleneck in a visual
  novel; text layout and asset loading are.
