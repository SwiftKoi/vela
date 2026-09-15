# Tooling Specification

Status: **draft, normative for M10–M12.**

Tooling is not a nice-to-have here — it is the product (`VISION.md §3.1`). The engine exists
so that a story can be checked, tested, refactored, and debugged like software. This document
specifies the surface a user actually touches.

## 1. CLI

Binary: `vela`. Subcommands are registry entries (`CONVENTIONS.md §4.7`).

| Command | Purpose | Notable flags |
| --- | --- | --- |
| `vela new <name>` | Scaffold a project | `--template minimal\|standard\|rich` |
| `vela run` | Run with hot reload | `--headless`, `--start <label>`, `--seed <n>` |
| `vela check` | Compile + lint, no run | `--deny-warnings`, `--format human\|json\|sarif`, `--watch` |
| `vela build` | Package for targets | `--target web,win,mac,linux,android`, `--release`, `--patch-from <prev>`, `--patch-out <dir>`, `--verify-reproducible` |
| `vela patch apply <patch> <bundle>` | Apply a delta patch to an existing build | — |
| `vela test` | Headless story tests | `--update`, `--seed`, `--filter`, `--a11y` |
| `vela analyze` | Story graph + asset reports | `--format dot\|json\|html`, `--dead-labels`, `--assets` |
| `vela fmt` | Canonical formatter | `--check`, `--diff` |
| `vela doc` | Generate docs from schemas | `--out docs/`, `--open` |
| `vela lsp` | Language server on stdio | `--stdio`, `--log <file>` |
| `vela debug` | DAP server | `--port`, `--stdio`, `--start <label>` |
| `vela migrate <path>` | Ren'Py → Vela transpiler | `--report`, `--in-place`, `--strict` |
| `vela doctor` | Environment diagnosis | `--verbose` |

Design rules:
- Every command supports `--format json` where output is machine-consumed.
- Exit codes are stable and documented: `0` success, `1` diagnostics present, `2` usage error,
  `3` internal error. CI depends on this and it never changes.
- `vela check` is the CI entry point; `vela run` is the developer entry point. They share the
  compiler and must never disagree.

## 2. Diagnostics contract

One model, three renderings (`ARCHITECTURE.md §6.1`). Codes are defined in
`LANGUAGE.md §8` and registered centrally; `check-diag-codes` enforces uniqueness, the range
scheme, and that every code has a test.

### 2.1 Human

```
error[E5003]: undefined label `forest.clearring`
  --> chapters/start.vela:42:10
   |
42 |     jump forest.clearring
   |          ^^^^^^^^^^^^^^^ no label `clearring` in module `forest`
   |
   = help: did you mean `forest.clearing`?
   = note: labels in `forest`: clearing, river, camp
```

Rules: always print the code, the primary span, and — where one exists — a machine-applicable
suggestion. "Did you mean" suggestions are edit-distance based over the *actually in-scope*
namespace, not a global list.

### 2.2 JSON
A stable, versioned schema: `{ code, severity, message, spans[], notes[], suggestions[] }`.
Stability is a compatibility promise; a consumer parsing this must not break on upgrades.

### 2.3 SARIF
`vela check --format sarif` uploads directly to GitHub code scanning, so every diagnostic
appears inline in a PR. This is the mechanism that makes "check your story like code" real
rather than aspirational.

### 2.4 Severity and promotion
Every diagnostic has a default severity. A project may promote or demote any code in
`vela.toml`:

```toml
[lints]
W4002 = "error"      # unreachable label — we want this to block merges
W7001 = "allow"      # unused asset — noisy during art production
```

`--deny-warnings` in CI treats all warnings as errors. Demoting an `E` to a warning is
rejected (`E7301`) — errors are errors.

## 3. Formatter

`vela fmt` produces one canonical form of every file. This is not cosmetic: it is what makes
diffs readable and merge conflicts rare in a project with many writers.

Rules:
- One statement per line, always (this mirrors `LANGUAGE.md §1.6`).
- 4-space indent; blank lines between labels/menus; single blank line inside long blocks.
- Right-hand side of `=` is wrapped at 88 columns by splitting at the *outermost* binary
  operator, never mid-token.
- Trailing commas in multi-line lists/maps/params; no trailing whitespace.
- Strings are never reflowed or re-escaped except to normalize redundant escapes.
- A `# fmt: off` pragma exists and is linted (`W4011`) so its use is visible and reviewed.

The formatter is deterministic and the compiler never depends on formatting — so a file that
fails `--check` is still compilable, and CI's `fmt --check` is a style gate, not a
correctness gate.

## 4. Language Server (`vela-lsp`)

A thin adapter over the incremental query database in `vela-compile`
(`ARCHITECTURE.md §6.2`). The server holds no analysis state of its own.

| Capability | Notes |
| --- | --- |
| Diagnostics (push) | Same codes as `vela check`; must be identical, verified by a parity test |
| Hover | Types of expressions, docs for labels/screens/widgets/effects |
| Completion | Labels (scoped, not global), screens + their params, widget names, props, enum variants, actions |
| Goto definition | Labels, defaults, structs/enums, characters, images, assets (`@path`) |
| Find references | Across modules — the reason namespacing matters |
| Rename | Safe cross-module rename of labels, defaults, types |
| Signature help | Labels and screens |
| Semantic tokens | Distinguishes labels, characters, defaults, types, assets |
| Inlay hints | Inferred types on `var`; assets resolved on `@path` |
| Code actions | Add missing `match` arms; add missing `use`; extract label; convert `absolute` layout to a container |
| Document symbols / workspace symbols | Labels, screens, types |

Two capabilities are worth calling out because nothing in the VN space has them:

- **Inlay hints for assets**: `@"assets/forest.png"` resolves and hints the imported size and
  format, catching a 12 MB PNG before it ships.
- **Story-aware code actions**: "this label is unreachable" offers to delete it; "this menu
  covers 2 of 4 branches" offers to scaffold the missing ones.

## 5. Test runner (`vela-test`)

Drives the real VM headless (§9 of `RUNTIME.md`). Tests live beside stories.

```vela
# tests/stories/forest.test.vela
test "picking the forest sets trust":
    run from forest.clearing
    choose "Explore"          # selects a menu option by text
    advance 4                 # consume 4 dialogue lines
    expect trust == 1
    expect visited(forest.river)

test "every route reaches an ending":
    cover labels             # asserts every label is executed
    cover variants           # asserts every enum variant is matched
```

Capabilities:

| Feature | Purpose |
| --- | --- |
| Scripted input | Deterministic choice/advance sequences (`RUNTIME.md §8`) |
| Seeded RNG | Reproducible randomness |
| Story-graph assertions | `cover labels`, `cover variants`, `no_dead_ends` |
| World assertions | Typed expressions evaluated against `World` |
| Command assertions | Assert the *exact* command stream (golden) or a shape (matcher) |
| Golden frames | Render at a scripted point, compare to a stored PNG (tolerance-based) |
| Locale sweep | Re-run the suite under every locale, failing on overflow/missing strings |
| Accessibility sweep | `--a11y` walks every screen's focus order |

`vela test --update` re-blesses goldens; CI fails on any unblessed change, so a golden diff
is always reviewed.

> **Implemented so far (M8).** Only the **accessibility sweep** exists — `vela test --a11y`
> walks every screen's focus order and *fails* a focusable node with nothing to announce, which
> is `W4010` enforced as a gate rather than reported as a warning. It is the M7 exit criterion
> *"every screen passes the `--a11y` focus-order sweep"*. The scripted-input, assertion, golden,
> and locale modes are M10's; `vela test` without `--a11y` says so rather than pretending to run
> them.

## 6. Debugger (DAP)

A Debug Adapter Protocol server, so debugging works in VS Code and any DAP client — Ren'Py has
no equivalent.

- Breakpoints on labels and on source lines.
- Step over / into / out across the frame stack.
- Inspect `World.defaults`, locals (named via `BYTECODE.md §5` debug slot names), call stack.
- Evaluate expressions in the paused frame using the real type checker — an invalid expression
  yields a normal diagnostic, never an evaluator crash.
- Time-travel: because rollback is snapshot-and-replay, the debugger can step *backwards*
  across commands. This is free from the runtime's design, not extra machinery.

## 7. Analysis (`vela analyze`)

Turns the compile-time story graph into human decisions.

| Report | Answers |
| --- | --- |
| Story graph | `--format dot\|json\|html` — the full branch structure as a navigable diagram |
| Dead labels | Labels no path can reach (`W4002`) |
| Dead ends | Paths that terminate without an ending |
| Branch coverage | Read the graph, not the docs: are all routes reachable? |
| Unused assets | Assets never referenced (`W7001`), with total size wasted |
| Asset budget | Per-scene load sizes — catches a 300 MB chapter before release |
| Localization coverage | Untranslated strings per locale, and UI overflow risk per locale |
| Variable reachability | `default`s never read, or read before ever written |

Output is deterministic and diffable, so `vela analyze --format json` can be tracked as a
metric over time in CI — "our dead-end count went up by 3 this week."

## 8. Migration (`vela-migrate`)

`vela migrate path/to/game` transpiles `.rpy` → `.vela` for the common case and reports the
rest precisely.

**Supported in the first pass (M12):** `label`, `jump`, `call`, `return`, say statements,
`menu`, `if/elif/else`, `define`/`default`, `character`, `image`, `show`/`hide`/`scene`/`with`,
basic `screen` blocks, `style`, transitions, and simple Python expressions that map to Vela
expressions (`$` statement blocks → best-effort function extraction, flagged).

**Reported, not guessed:** anything outside that set produces a report entry with
`file:line`, the original text, and the reason. The rule is simple:

> **Never silently mistranslate.** A wrong automatic translation is worse than an explicit
> "port this by hand", because it fails later and in a save file.

The report is itself a work item list, and `--strict` turns any unsupported construct into a
non-zero exit, so a team can track migration progress as a number that goes to zero.

## 9. Docs (`vela doc`)

Generates reference documentation from the single sources of truth already in the codebase:
widget prop schemas, effect signatures, diagnostic registry, action registry, and public
script APIs. Because it reads the same schemas the compiler uses, generated docs cannot drift
from behavior. This is also what keeps the LSP completion list and the docs identical.

## 10. Open questions

1. **Editor beyond LSP** (post-1.0): the GUI editor (`VISION.md §3.6`) is deliberately not in
   the 1.0 scope. The LSP plus the formatter is the 1.0 contract; the editor is a separate
   product built on it.
2. **Snapshot testing of text** (M10): golden frames cover rendering; deciding whether
   per-line text layout goldens are worth their maintenance cost.
3. **Distributed/shared `vela check` cache** (post-1.0): the query database is local; a shared
   CI cache is a possible speedup, not a design dependency.
