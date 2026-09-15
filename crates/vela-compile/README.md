# vela-compile

The compile driver: the incremental query database that ties the front end together.

**Owns:** Session, tracked queries, caching, lint passes, CompileResult.

**Does not own:** Language semantics; each phase crate owns its own.

Rank `6`. See `docs/ARCHITECTURE.md §1`.
