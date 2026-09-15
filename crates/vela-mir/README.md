# vela-mir

The typed mid-level IR - the stable compilation contract - and its optimization passes.

**Owns:** Module, Body, blocks, terminators, the pass pipeline registry, and **linking** a
program's modules into one (`link`, `LANGUAGE.md §6.1`).

**Does not own:** Encoding (vela-bytecode); checking (vela-types).

Rank `5`. See `docs/ARCHITECTURE.md §1`.
