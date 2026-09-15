# vela-span

Byte offsets, spans, file ids, and the source map that resolves offsets to line/column positions.

**Owns:** Span arithmetic, FileId allocation, SourceMap line indexing.

**Does not own:** Diagnostics or rendering (vela-diag); lexing (vela-syntax).

Rank `0`. See `docs/ARCHITECTURE.md §1`.
