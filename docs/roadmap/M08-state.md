# M8 — Save, rollback, and replay

**Goal.** The determinism bet pays off in player-visible behavior.

**Depends on.** M5.

**Crates.** `vela-replay`, `vela-world` (schema derivation).

**Work items.**
1. Schema derivation from `default`/`struct`/`enum` declarations; `schema_digest` computation.
2. Serialization of `World` + frames; `Save` container with atomic write (`RUNTIME.md §5`).
3. Snapshot ring buffer and rollback-to-command (`RUNTIME.md §7`).
4. Rewind-and-rebranch: discarding the log tail and continuing with a new input.
5. Migration engine + `migration!` macro (`RUNTIME.md §6.1`).
6. The append-only `tests/golden/saves/` corpus, seeded from M8 onward.
7. Snapshot-cost benchmark against the budget in `RUNTIME.md §7.2`.

**Exit criteria.**
- [x] Save in build N loads in build N+1 with only an added migration; asserted in CI
- [x] A missing migration fails with `E7201` and names the gap
- [x] Rollback to any of 20 points in a session replays exactly (asserted on `World` bytes)
- [x] A truncated save file is detected by checksum and the previous slot survives
- [x] Snapshot benchmark within budget on the standard fixture
- [x] **Demo:** `vela run examples/standard` — rollback wheel, save/load slots, migrate a
      fixture save across one version

**Where it landed.** `SAVE_VERSION` is 3. Two steps are in `vela-replay`'s migration chain —
`1 → 2` (`trust` → `affection`) and `2 → 3`, which is the suspension anchor and rewrites no
world state — and `tests/golden/saves/` holds a real save for each, so CI loads all three into
the current build on every run. `cargo xtask budget` measures the snapshot against
`RUNTIME.md §7.2`'s 1 ms and refuses to run in a debug build.

A save now records the *statement* a frame is suspended at, not just its instruction index
(`vela_vm::Resume`). A frame name survives a rebuild; so does a source range, and an index does
not — the same source at a different optimization level lays its instructions out differently.
A save whose story changed under it is refused with `Fault::StaleFrame` rather than resumed at
whatever moved into place. `crates/vela-vm/tests/snapshots.rs` builds one story at two
optimization levels and checks both halves: the suspension is found after the body moves, and a
line inserted before it is refused.

Two honest deviations from this page's wording:

- The snapshot benchmark builds the *spec's* scenario — a world with 10k `default` values —
  rather than measuring `examples/standard`, which declares no defaults at all and so could not
  exhibit the cost being budgeted.
- The demo's migration half is shown twice, neither time by loading the *fixture* save into
  `examples/standard`: a save belongs to the project whose schema and bodies it was written
  against, and the fixture's does not match `standard`'s. `examples/standard` loads a
  version-1 save of itself, and the fixture project loads the corpus's real version-1 file.
  Both report `load quick (migrated 1 -> 3)`; the world the second one produces is asserted by
  `tests/corpus.rs`, since the CLI prints the version and not the state.

**Risks.** Serialization formats ossify. Mitigation: the append-only save corpus is
established the moment the format exists, making incompatibility a test failure rather than a
player report.

**Still open after M8.** A rollback or load re-applies the command on screen but does not
rebuild the staged scene from the restored `World`, so a rollback across a `scene` change
leaves the old backdrop until the next one. Snapshot intervals do not yet auto-tune for a
project over budget. The digest stays deliberately non-cryptographic. A save written before the
anchor existed (the corpus's version-1 and version-2 files) still resumes from its index, which
catches a body that is too short and nothing subtler.
