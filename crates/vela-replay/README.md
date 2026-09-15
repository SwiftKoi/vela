# vela-replay

Snapshots, rollback, the input log, and the save migration engine.

**Owns:** Recorder, Snapshot, Migrator, save serialization, the `migration!` chain, atomic
writes.

**Does not own:** The interpreter (vela-vm); the state model (vela-world).

Rank `8`. See `docs/ARCHITECTURE.md §1`.

`tests/golden/saves/` (at the workspace root) is this crate's corpus: one real save per format
version, loaded by `tests/corpus.rs` on every CI run. `RUNTIME.md §6.3`.
