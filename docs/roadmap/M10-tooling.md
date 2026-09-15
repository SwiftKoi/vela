# M10 — Tooling: LSP, formatter, tests, analysis

**Goal.** The differentiator becomes usable in an editor and in CI.

**Depends on.** M2 (can start early), M5, M7 for goldens.

**Crates.** `vela-lsp`, formatter (landed in `vela-syntax`), `vela-test`.

**Landed early: the formatter.** Item 1's printer and `vela fmt` / `--check` are in, because
nothing else can *establish* the syntax before this milestone: a formatter is what turns "these are
the rules" into text that is checked on every file in the repository. It is in `vela-syntax`
(`format`, `print/`) since it is a function of the tree, and it is pinned by two corpus-wide tests —
`crates/vela-syntax/tests/format_roundtrip.rs` (idempotent, re-parses, keeps every comment) and
`crates/vela-mir/tests/format_equivalence.rs` (lowers to the same program before and after).
`TOOLING.md §3` records the canonical form, including the choices the tree cannot make for itself.
**Item 1 is done except two pieces.** `vela fmt --check` over this repository's own corpus is now a
gate (`check-format`, `REPO_LAYOUT.md §4`): 46 files canonical, and the 15 that do not parse are the
`E0xxx`/`E1xxx` fixtures, which are *meant* to be broken and are counted rather than failed. Running
it by hand the first time found a language bug, not a formatting nit — `not` and `!` had been arriving
in the tree as one variant although they bind at different levels (`not a == b` is not `!a == b`), so
the formatter could not print either faithfully. Still open here: `--diff`, and the `W4011` lint that
makes a `# fmt: off` region visible in review.

**Input from the Ren'Py reference.** Ren'Py's `developer_tools` and `cli` pages are the closest
thing to a spec for this milestone, and four of its tools are worth copying rather than inventing:

- `renpy lint` — a check list to steal wholesale for `vela analyze`: labels that are never reached,
  a say for an undeclared character, images declared and never used, assets referenced and missing,
  and *a `call` with no `from` clause* (which is how Ren'Py's own build finds the saves that are
  about to break; see `RUNTIME.md §5`).
- `renpy translate` — extraction of translatable strings into a catalogue. Two decisions come with
  it: dialogue is translatable *by default* (no marker in the script), and the message id has to be
  edit-stable, which is the same problem as the save anchor.
- `renpy --warp <label>` — start the story at a label and skip the rest. Cheap, and it is what makes
  a long fixture tolerable to work on by hand.
- `testcase` blocks (`run`, `click`, `type`, `assert`) — a shape worth matching in `vela-test` so it
  reads as familiar to anyone who has written one, even though the runner is ours.

**Work items.**
1. Formatter with the rules in `TOOLING.md §3`; `--check` and `--diff`.
2. LSP server over the query database; the capability list in `TOOLING.md §4`.
3. Diagnostic parity test: LSP diagnostics must equal `vela check` output exactly.
4. `vela-test`: scripted input, world assertions, command assertions, story-graph assertions,
   locale sweep, accessibility sweep (`TOOLING.md §5`).
5. Golden frame rendering with tolerance.
6. `vela analyze`: story graph (`dot`/`json`/`html`), dead labels/ends, unused assets, asset
   budget, variable reachability (`TOOLING.md §7`).
7. `vela doc` generating widget/effect/action/diagnostic references from their schemas.

**Exit criteria.**
- [ ] Hover, completion, goto-def, references, rename all work across module boundaries in a
      multi-module fixture
- [ ] `vela fmt --check` on the entire corpus is clean and idempotent
- [ ] `vela test` runs a suite headless in CI and fails a deliberately broken story
- [ ] `vela analyze --format json` output is deterministic across runs (diffed in CI)
- [ ] A newcomer can navigate the fixture using only LSP features (documented walkthrough)
- [ ] **Demo:** the LSP walkthrough in the README, plus `vela test && vela analyze`

**Risks.** LSP correctness depends entirely on the M2 query database being right. Mitigation:
the parity test makes any divergence between editor and CLI a hard failure.
