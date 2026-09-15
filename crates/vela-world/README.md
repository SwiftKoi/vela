# vela-world

Runtime state: typed values, entities, the deterministic RNG, and the serialization schema.

**Owns:** World, Value, schema derivation, pinned float formatting.

**Does not own:** Snapshots and save files (vela-replay); the interpreter (vela-vm).

Rank `1`. See `docs/ARCHITECTURE.md §1`.
