# vela-hir

Name resolution, scopes, the desugared AST, and the story graph.

**Owns:** Hir, DefId, scopes, use/pub handling, StoryGraph construction.

**Does not own:** Types (vela-types); lowering to IR (vela-mir).

Rank `2`. See `docs/ARCHITECTURE.md §1`.
