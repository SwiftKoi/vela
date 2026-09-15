# vela-types

Types, inference, and the checking rules.

**Owns:** Ty, TypeCtx, inference, exhaustiveness and reachability analysis.

**Does not own:** Name resolution (vela-hir); code generation (vela-mir).

Rank `3`. See `docs/ARCHITECTURE.md §1`.
