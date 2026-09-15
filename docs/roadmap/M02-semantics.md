# M2 — Semantics: names, types, story graph

**Goal.** A program that parses is checked: every name resolves, every type agrees, and the
story graph is analyzed.

**Depends on.** M1.

**Crates.** `vela-hir`, `vela-types`, `vela-compile` (query database).

**Work items.**
1. Query database (salsa-style) with tracked inputs `file`, `module`; queries `parse`,
   `resolve`, `typecheck`. **Design this in now, not later** (`ARCHITECTURE.md §6.2`).
2. Name resolution: module scopes, `use`, `pub`, duplicate detection (`E2xxx`).
3. Type representation and inference for `var` (`LANGUAGE.md §5`); explicit-signature rules.
4. Type checker split by construct: `check/stmt/`, `check/expr/`, `check/call.rs`.
5. `StoryGraph` construction from `label`/`jump`/`call`/`menu`; unreachable and unterminated
   label analysis (`W4002`, `W4003`).
6. Exhaustiveness and reachability (`E4001`, `W4005`) for `match` and `if`.
7. Type-related diagnostics `E3xxx`, story diagnostics `E5xxx`.
8. `vela check --format json` and `--format sarif` with a stable schema (`TOOLING.md §2`).

**Exit criteria.**
- [x] Every `E2xxx`–`E5xxx` code in `LANGUAGE.md §8` has a test
- [x] `W4002` fires on an unreachable label in a fixture and not on a reachable one
- [x] SARIF output validates against the SARIF schema — checked against the official 2.1.0
      schema, and CI now validates it on every push. *Uploading* is the exception:
      `upload-sarif` needs code scanning enabled, the same plan limitation as branch
      protection, so it is a repository setting rather than something this milestone can
      deliver.
- [x] Incremental proof: editing one label in a 200-label fixture re-checks only its module,
      asserted via query-invocation counts (and editing an *imported* module re-checks its
      importer)
- [x] **Demo:** `vela check --deny-warnings` on a fixture with six deliberate errors reports
      all six and two warnings, and exits 1

**Status.** Complete. 184 tests, 7/7 `xtask` checks, clippy clean with `-D warnings`.

**Found during implementation.**

- **Two cache-correctness bugs**, both caught by the incrementality tests and both the same
  shape in reverse. `collect_deps` *took* the scratch entries, so a nested query emptied its
  caller's dependency set — and a query with no dependencies is always "current", so it was
  never invalidated and silently served a stale result. Then a cache *hit* failed to record
  its input, so a caller that found the answer already computed was never invalidated when
  the file it came from changed. Both present as "an edit did not take effect", which is the
  hardest kind of bug to attribute.
- **A `use` of a module that does not exist did nothing at all.** The alias pointed at
  nothing, so every qualified reference through it reported a *missing* `use` — the wrong
  diagnosis, on the wrong line, with the actual typo never named.
- **`E3001` was unreachable.** An enum with no body was a *syntax* error, so the parser
  reported a missing block before the type checker could report the empty enum. An enum's
  body is now optional, because "this enum has no variants" is a type error.
- **The human summary corrupted machine output.** `--format json` emitted the document and
  then appended "6 error(s), 2 warning(s)", which made it unparseable. Only found by piping
  it into a parser.
- **The JSON and SARIF shapes did not match `TOOLING.md §2.2`** where it was specific: a
  suggestion belongs in its own entry, not folded into a span.

**Known limitation, stated rather than hidden.** A qualified type name from another module
lowers to unknown, so cross-module *types* are not checked. Everything else is per-module,
which is what keeps checking inside the module's own query. Names and labels do cross
modules; types do not yet.

**Risks.** Inference and lowering were expected to be the danger; they were routine. The
danger was in the *cache*, which the milestone's own exit criteria caught twice — worth
noting for M3, where the same machinery carries more queries.
