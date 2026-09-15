# vela-mir

The typed mid-level IR - the stable compilation contract - and its optimization passes.

**Owns:** Mir, Body, blocks, terminators, the pass pipeline registry.

**Does not own:** Encoding (vela-bytecode); checking (vela-types).

Rank `4`. See `docs/ARCHITECTURE.md §1`.
