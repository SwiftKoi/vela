# M10 — Tooling: LSP, formatter, tests, analysis

**Goal.** The differentiator becomes usable in an editor and in CI.

**Depends on.** M2 (can start early), M5, M7 for goldens.

**Crates.** `vela-lsp`, formatter (in `vela-syntax` or its own crate), `vela-test`.

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
