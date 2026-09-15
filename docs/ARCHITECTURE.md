# Architecture

## 1. Shape

Vela is a Rust workspace with a one-directional dependency graph, enforced in CI
(`cargo xtask check-layers`) rather than by convention — conventions erode, and a 3k-line file
starts life as five reasonable ones.

**Every crate declares an integer `rank`.** A crate may depend only on crates of a strictly
lower rank. Ranks are fine-grained because the front end is a chain
(`span → syntax → hir → types → mir → bytecode`); a coarse four-layer scheme cannot express a
linear chain without permitting same-rank edges, and same-rank edges are exactly the ones that
grow into cycles.

```
rank  crates
────  ───────────────────────────────────────────────────────────────────────
 0    vela-span
 1    vela-diag     vela-world   vela-text    vela-audio   vela-host*
 2    vela-syntax   vela-render* vela-assets
 3    vela-hir
 4    vela-types
 5    vela-mir
 6    vela-bytecode
 7    vela-compile  vela-vm
 8    vela-replay   vela-ui      vela-lsp     vela-migrate
 9    vela-test     vela-plugin
10    vela-cli     vela-web*
```

**Rule 1 — rank.** A crate may depend only on crates with a *strictly lower* rank. Edges
never point up or sideways.

**Rule 2 — adapters.** `vela-host` (windowing, input, filesystem, clock) and `vela-render`
(GPU) are marked `*`: they are *adapters*. **No crate of rank ≤ 7 may depend on an adapter.**
Ranks 8–10 may — `vela-ui` consumes the renderer, `vela-cli` consumes the platform host.

Why this shape: Rule 2 is what keeps the front end, compiler, and VM free of both graphics
and platform code. `vela test` therefore runs the entire story engine headless on a build
server, using the *same* VM the game uses — not a reimplementation. And the compiler can run
on `wasm32`, which is what makes the web target's in-browser tooling possible.

**Note on `vela-diag` and `vela-syntax`.** Every phase emits diagnostics, so `vela-syntax`
(and `vela-hir`, and `vela-types`) depends on `vela-diag`. That is why `vela-diag` sits
directly above `vela-span` rather than beside `vela-syntax`: a shared diagnostic model has to
be below everything that reports through it.

## 2. Crate map

| Crate | Rank | Responsibility | Key public types |
| --- | --- | --- | --- |
| `vela-span` | 0 | Byte offsets, spans, line/col, file ids | `Span`, `FileId`, `SourceMap` |
| `vela-syntax` | 2 | Lexer, parser (CST), token kinds | `Cst`, `Token`, `Parser` |
| `vela-diag` | 1 | Diagnostic model, codes, rendering (human + JSON + SARIF) | `Diagnostic`, `Code`, `Emitter` |
| `vela-world` | 1 | Runtime state: typed values, presentation commands, entities, RNG, serialization schema | `World`, `Value`, `Command` |
| `vela-text` | 1 | Shaping, layout, glyph atlas, font subsetting hooks | `ShapedRun`, `TextLayout` |
| `vela-audio` | 1 | Audio graph description (mixing is a host responsibility) | `AudioCommand`, `Bus` |
| `vela-host` | 1 | **adapter** — platform traits + native impls (window, input, fs, time) | `Host`, `InputEvent`, `Clock` |
| `vela-hir` | 3 | Name resolution, scopes, desugared AST, story graph | `Hir`, `DefId`, `StoryGraph` |
| `vela-render` | 2 | **adapter** — `wgpu` renderer, render graph, shaders, command consumption | `Renderer`, `RenderGraph` |
| `vela-assets` | 2 | Importers, transformers, content-addressed manifest | `Importer`, `Manifest`, `Digest` |
| `vela-types` | 4 | Type representation, inference, checking rules | `Ty`, `TypeCtx`, `Infer` |
| `vela-mir` | 5 | Typed mid-level IR (the stable compilation contract), optimization passes, and a reference interpreter | `Module`, `Body`, `Stmt` |
| `vela-bytecode` | 6 | Instruction set, assembler/disassembler, verifier, `.velac` codec | `Op`, `Module`, `Verifier` |
| `vela-compile` | 7 | Compile driver, incremental query database, session | `Session`, `Query`, `CompileResult` |
| `vela-vm` | 7 | Bytecode interpreter, deterministic scheduler, yields commands | `Vm`, `Frame`, `Step` |
| `vela-replay` | 8 | Snapshots, rollback, input log, save migration engine | `Recorder`, `Snapshot`, `Migrator` |
| `vela-ui` | 8 | Screen runtime, layout solver, widget registry, styling | `Tree`, `Widget`, `Registry` |
| `vela-lsp` | 8 | Language server over `vela-compile` | `Server` |
| `vela-migrate` | 8 | `.rpy` → `.vela` transpiler + compat report | `Transpile`, `Report` |
| `vela-test` | 9 | Headless story runner, assertions, golden frames | `StoryTest`, `Asserts` |
| `vela-plugin` | 9 | WASM plugin host, capability ABI, versioned surface | `PluginHost`, `Capability` |
| `vela-cli` | 10 | The `vela` binary; subcommand registry | `Command`, `Main` |
| `vela-web` | 10 | **adapter** — the wasm module a browser loads | `Player` |

Crates are introduced by milestone; see `ROADMAP.md`. `vela-text`, `vela-audio`, and
`vela-render` are stubs until M6 — the compiler and VM must be fully testable without them.

**`Command` lives in `vela-world`, not beside the VM.** The *compiler* constructs these
values when it lowers `say` and `show`, so they have to be usable below rank 5 — and a
command is close kin to the scene state it leaves behind. Both are presentation as plain
data.

**`vela-mir` carries a reference interpreter**, and deliberately not a second VM. MIR is a
contract, and a contract with no executable meaning is prose; the interpreter is that
meaning in runnable form and the oracle the optimizer is tested against. It is a direct
evaluator over MIR rather than a bytecode VM so that a pass bug cannot hide behind a codegen
bug that happens to cancel it out. `vela-vm` (M5) is the engine — opaque bytecode, effect
scheduling, an input log, and snapshots.

## 3. Data flow

Two pipelines share the same source files. This is deliberate: the editor and the build
must never disagree about what a program means.

### 3.1 Build pipeline (ahead-of-time)

```
.vela source
   │  vela-syntax::lex + parse
   ▼
CST ──────────────► vela-diag (syntax errors E0xxx/E1xxx)
   │  vela-hir::lower
   ▼
HIR ──► name resolution ──────► E2xxx, story graph checks E5xxx
   │  vela-types::check
   ▼
typed HIR ────────────────────► E3xxx, exhaustiveness/reachability E4xxx
   │  vela-mir::lower
   ▼
MIR (typed IR, the stable contract)
   │  vela-mir::opt (pass pipeline — extensible)
   ▼
optimized MIR
   │  vela-bytecode::codegen
   ▼
bytecode module ──► verifier (E6xxx if a pass is buggy) ──► .velac bundle
                                                            (bytecode + asset manifest
                                                             + schema hashes)
```

### 3.2 Editor pipeline (interactive)

Same front end, driven by an incremental query database in `vela-compile`. Edits
invalidate only the queries downstream of the changed syntax node, which is what makes
completion and diagnostics feel instant on a 500k-word project. The LSP is a thin adapter
over that database — it holds no state of its own.

### 3.3 Runtime loop

```
.velac bundle ──► vela-vm::load ──► Vm
                                     │  step() until a yield
                                     ▼
                                  Command  (Say, ShowScreen, Play, Wait, …)
                                     │
                       ┌─────────────┴──────────────┐
                       ▼                            ▼
                 vela-ui / vela-render        vela-test (headless)
                 (real presentation)          (assert on commands & World)
                       │
                       ▼
                 InputEvent ──► recorded in the input log ──► back into Vm
```

The critical property: **the VM never touches the renderer.** It emits declarative
`Command`s and consumes `Event`s. That single boundary is what makes headless testing,
deterministic replay, and save/restore all fall out of the same design instead of being
three separate subsystems.

## 4. Determinism model

Determinism is an invariant. It is enforced, not hoped for.

**Sources of nondeterminism, and their treatment:**

| Source | Rule |
| --- | --- |
| Wall clock | Never read directly. Time is a host value injected into `World` and advanced by explicit ticks. |
| `HashMap` iteration | Banned in any crate that affects output. `IndexMap`/`BTreeMap` only; CI lint is `clippy::disallowed_types`. |
| RNG | One seeded generator inside `World`; advanced only by the scripted `rand` effect. |
| Float formatting | Pinned formatting helpers; no locale-dependent output. |
| Thread scheduling | VM is single-threaded and synchronous. Parallelism is allowed only in asset build, which is content-addressed and order-independent. |
| Pointer identities | No `as usize` address identity anywhere in state. Entities use generational ids. |
| Host capability results | Every host effect result is recorded in the input log. |

**Consequences.** Rollback = restore nearest snapshot + replay the input log to the target
point. Save = serialize `World` + VM frames (both are plain data). A cross-version save is
just an older schema plus a chain of migrations.

## 5. Extension points

The requirement is explicit: **new capability must not mean editing a core file.** Every
extension surface is a registry populated at link/startup time.

| To add… | Register with | Lives in | Core files touched |
| --- | --- | --- | --- |
| A new statement/expression | Parser rule table + pass pipeline | `vela-syntax`, `vela-hir` | **None** — rules are added to a table module |
| A new effect (host call) | `EffectRegistry` | `vela-vm` / `vela-host` | **None** |
| A new UI widget | `WidgetRegistry` | `vela-ui` | **None** |
| A new asset format | `ImporterRegistry` | `vela-assets` | **None** |
| A new lint | `PassRegistry` | `vela-compile` | **None** |
| A new render stage | `RenderGraph` insertion | `vela-render` | **None** |
| A new CLI command | `CommandRegistry` | `vela-cli` | **None** |
| A new bytecode instruction | `OpSpec` table + handler struct | `vela-bytecode`, `vela-vm` | **One table module** |
| A platform backend | `Host` trait impl | `vela-host` | **None** |

The eighth row is the deliberate exception: the instruction set is a *closed* set for
verification and performance reasons, so adding an op touches exactly one table file and one
handler file — both under the line budget. Everything else is fully open.

**The extension matrix in `CONVENTIONS.md` expands each row into a concrete, ordered
checklist** of files and tests. That document is the contract that keeps this table honest.

## 6. Cross-cutting concerns

### 6.1 Diagnostics
One diagnostic type, one code registry, three renderers (human, JSON, SARIF). Every
diagnostic carries a stable code, a primary span, optional labeled spans, and a machine-
applicable suggestion where one exists. Codes are never reused or renumbered. See
`spec/LANGUAGE.md §8` for the numbering scheme and `spec/TOOLING.md §2` for renderings.

### 6.2 Incremental computation
`vela-compile` is built on a query database (salsa-style): `parse(file)`, `resolve(module)`,
`typecheck(body)`, `mir(body)`, `codegen(body)` are queries with tracked inputs. This is not
an optimization — it is what makes the LSP viable, and it is cheapest to design in from the
start rather than retrofit.

### 6.3 Capability-based host
Scripts cannot reach the OS. They call effects; effects are declared capabilities
(`fs.read_save`, `audio.play`, `input.wait`). Production injects real implementations;
tests inject mocks; plugins receive a restricted subset. This is the mechanism behind both
sandboxing and headless testing.

### 6.4 Plugin ABI
Plugins are WASM modules (wasmtime) speaking a versioned ABI. They may register into any
registry listed in §5 and receive only the capabilities they declare. The ABI is versioned
and additive; a plugin built against ABI 1 keeps working when we add ABI 2 features.

## 7. Key decisions

Short decision records. Each has a rationale and, where relevant, the alternative we
rejected. Long-form decisions graduate to a file under [`docs/adr/`](adr/) when they are
revisited; the index there is the canonical list.

| # | Decision | Rationale | Rejected alternative |
| --- | --- | --- | --- |
| D1 | Rust workspace, many small crates | Enables the file-size budget (Principle 4) and layer enforcement | Single crate with modules — no enforcement, tends monolithic |
| D2 | Indentation-significant syntax | Familiar to the audience we want to win | Braces — cheaper to parse, worse ergonomics for prose-heavy scripts |
| D3 | Bare string literal = say statement | Preserves Ren'Py's one-line greeting | Explicit `say "..."` — loses the magic |
| D4 | Compile to bytecode, no embedded Python | Analyzability is the whole point | Embed a GC'd language — reintroduces unanalyzable dynamism |
| D5 | MIR as the stable contract; stack bytecode v1 | Lets us add a register backend later without changing front ends | Emit machine-ish code directly — premature optimization, unstable |
| D6 | Determinism enforced by lints, not discipline | Discipline fails under deadline | Advisory docs only |
| D7 | Declarative `Command` boundary between VM and presentation | Headless testing + replay + save all derive from it | VM calls renderer directly — fast to start, dead end |
| D8 | Web-first via `wgpu`/wasm | Discovery and instant demos matter | Desktop-first — web becomes a rewrite later |
| D9 | Registry-based extension everywhere | Satisfies "never edit a core file" | Trait objects hardcoded at call sites |
| D10 | Ren'Py compat via transpiler, not fork | We need to fix the DSL, not inherit it | Fork/compat shim at the language level |

## 8. Performance posture

We optimize **predictability over peak throughput**, with two exceptions that are worth the
complexity because they are user-visible on every frame and every launch:

- **Startup**: bytecode loads without recompilation; asset manifest is memory-mappable; no
  scripting-language warmup. This is now a path rather than an intention: `vela_vm::Session::load`
  takes a built bundle to the machine, and `vela run <bundle>` runs one with no compiler in the
  path (`RUNTIME.md §8`, `BUILD_AND_ASSETS.md §8`).
- **Frame time**: text layout and glyph atlas updates are cached and invalidated by change,
  not recomputed per frame. The UI tree is diffed on dirty flags, never fully relaid out.

Everything else — VM speed, shader throughput — is "fast enough" until a measurement says
otherwise. Prematurely optimizing the VM is how a project spends six months and ships
nothing.
