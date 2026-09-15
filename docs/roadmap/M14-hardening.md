# M14 — 1.0 hardening

**Goal.** Everything in `VISION.md §5` is measurably true.

**Depends on.** All.

**Crates.** All.

**Work items.**
1. Performance pass against the budgets: startup, per-command, snapshot, frame time, wasm size.
2. Reproducibility and cross-platform determinism matrices finalised (three architectures).
3. Fuzz campaigns: random inputs replayed; verifier fuzzing; malformed `.velac` handling.
4. Save-corpus audit: every historical version loads; migration coverage reported.
5. Documentation completeness: every diagnostic, widget, effect, action, and CLI flag
   documented and generated (`vela doc`).
6. Stability: no panics on any malformed input, verified by a fuzz corpus and a panic-hook
   test harness.
7. Release engineering: versioning policy, upgrade guide, changelog automation.

**Exit criteria.** Each maps to a claim in `VISION.md §5`.
- [ ] `vela new` → `vela run` with zero configuration files required
- [ ] All seven "caught before execution" diagnostic classes are demonstrably caught
- [ ] A save survives three engine versions with only added migrations
- [ ] Rollback is exact under replay, including RNG
- [ ] Widget/effect/importer/lint extension requires zero core edits (extension matrix audit)
- [ ] No file over budget anywhere in the workspace
- [ ] A text-only patch is < 5% of the bundle
- [ ] **Demo:** the release candidate, plus the full gate suite green

**Risks.** "Hardening" is where scope quietly expands. Mitigation: this milestone has no new
features — any new feature is deferred to post-1.0, and the backlog is where it goes.
