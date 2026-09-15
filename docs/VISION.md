# Vision

## 1. The problem

Ren'Py solved *expressiveness*. Anyone can write:

```vela
label start:
    "Hello, world."
```

and get a game on Steam. That is a genuine achievement and we must not lose it.

Ren'Py did **not** solve *developability*. Its script language is dynamically interpreted and
therefore fundamentally unanalyzable. The consequences compound with project size:

| What hurts | Why Ren'Py can't fix it | Vela's answer |
| --- | --- | --- |
| Typos crash at runtime | No static analysis of labels/screens/vars | Compile-time resolution + LSP |
| Refactors are terrifying | No find-references, no rename, flat global label namespace | Namespaced modules, real rename |
| "Which scene is dead?" | No story graph tooling | `vela analyze` extracts the graph |
| Patching a shipped game breaks saves | Save = pickled interpreter internals | Save = versioned schema + migrations |
| Merge conflicts in GUI/screens | Generated files + inconsistent formatting | `vela fmt`, one statement per line |
| Story logic can't be tested | No headless harness | `vela test` + deterministic replay |
| Engines are hard to extend deeply | Monolithic Python internals | Small crates, registry-based extension |
| Builds are monolithic | No content addressing | Chunk-addressed **delta patches** |

The unifying root cause: **the script layer is not a program you can reason about.**
Everything above follows from that one fact. Fix it and the rest becomes tractable.

## 2. Principles

Ordered. When two conflict, the lower number wins.

1. **Simple stays simple.** One line of dialogue is one line. Power is opt-in and additive.
2. **Everything is analyzable.** If a tool cannot reason about it, it does not ship.
3. **Determinism is an invariant, not a feature.** Same save + same inputs = same outcome,
   bit for bit, across platforms and versions.
4. **Small modules over big files.** A file we cannot read in one sitting is a bug. See the
   500-line budget in `engineering/CONVENTIONS.md`.
5. **The script is the source of truth.** The editor, the formatter, and the debugger all
   operate on the same files a human hand-writes. No hidden project format.
6. **Extension is a first-class API.** Adding a widget, effect, importer, or lint must never
   require editing a core file.
7. **Deterministic tooling.** Formatter, compiler, and build are reproducible. No wall-clock,
   no unordered iteration, no ambient nondeterminism.
8. **Accessibility is structural.** It is not a mode bolted on at the end.
9. **The host is a capability.** Scripts reach the outside world only through declared,
   mockable interfaces.
10. **Ship the boring thing.** Save/load, text rendering, and input must be flawless before
    we chase spectacle.

## 3. Differentiators (why anyone switches)

These are ordered by how hard they are to copy. The first one is the reason to exist.

### 3.1 Typed story language + LSP + headless test runner
The engine compiles scripts to bytecode with real diagnostics (`file:line:col`, stable error
codes) instead of an interpreter traceback. An LSP surfaces undefined labels, bad screen
arguments, and non-exhaustive matches in the editor. A headless runner walks the story graph
and asserts on reachability, branch coverage, and final state — so stories get CI.

### 3.2 Deterministic saves, rollback, and replay
Runtime state is plain, typed, serializable data plus an ordered input log. Rollback is
snapshot-and-replay. Save compatibility across engine versions is handled by **declarative
migrations** rather than "we broke your saves."

### 3.3 Split story flow from the general-purpose language
Ren'Py conflates them: Python is simultaneously the dialogue language and the engine API.
Vela has a small, sandboxable bytecode VM for story flow and a capability-based host
interface for everything else. That buys reliable hot reload, deterministic testing, and
WASM plugin sandboxing.

### 3.4 Web-first, one codebase
`vela build --target web,win,mac,linux,android`. Instant browser demos matter for discovery;
that path must be first-class, not an afterthought.

### 3.5 Content-addressed builds with delta patches
Assets are chunk-addressed and hashed. Shipping a typo fix to a 2 GB game should download
kilobytes, not gigabytes.

### 3.6 A GUI editor over the same files you hand-write
The editor assists; it never introduces a separate project format. Code and WYSIWYG cannot
diverge because there is only one artifact. This is post-1.0 (see the roadmap backlog) and
depends on the LSP and the formatter existing first — but it is a differentiator, so the
tooling is designed to make it possible rather than to block it.

## 4. Non-goals

Scope discipline. Saying no here is what makes the roadmap finishable.

- **Not a general-purpose game engine.** No physics simulation, no action gameplay. The 3D
  stage exists for cameras, planes, and models in a VN context.
- **Not embed-Python-in-v1.** Host-language FFI is a later, additive capability — not a
  foundation.
- **Not a live-service platform.** No accounts, no marketplace, no cloud in v1.
- **Not a drag-and-drop-only tool.** The editor assists; the text format is canonical.
- **Not a Ren'Py fork.** Compatibility arrives via a migration transpiler, not a shared
  codebase. We will not inherit the DSL we are trying to fix.

## 5. What "better" means, measurably

Vague ambition produces vague work. These are the acceptance bars for a 1.0, each one
checkable by a test:

- A beginner's first project is created and run by `vela new` + `vela run`, with **no
  configuration files required**.
- The full diagnostic suite catches, **before execution**: undefined labels, unreachable
  labels, undefined variables, type mismatches, non-exhaustive matches, undefined screens,
  and bad screen arguments.
- A save produced by build *N* loads in build *N+3* with no game-code changes, given only
  added migrations.
- Rollback from any point to any earlier point in a session is exact under replay,
  including RNG.
- Adding a new widget, effect, asset importer, or lint requires **zero edits** to any core
  crate (verified by the extension matrix and an `xtask` check).
- No source file in the workspace exceeds the line budget, enforced in CI.
- A patch release downloads **< 5%** of the full build size for a text-only change.

## 6. Adoption

Nobody rewrites a shipped VN. Adoption is a ladder:

1. **New small projects** — "I just want to make a VN and not fight my tools."
2. **Jam and prototype projects** — the web target matters here.
3. **Serious mid-size projects** — tooling and testing start paying for themselves.
4. **Existing Ren'Py projects** — `vela migrate` transpiles the common 90% of `.rpy`,
   reports the rest with line references, and lets teams port incrementally.

Step 4 is a *destination*, not a starting point. It ships late (M12) precisely because it is
only credible once the rest of the engine is real.
