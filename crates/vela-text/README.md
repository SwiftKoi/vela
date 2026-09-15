# vela-text

Text shaping, line breaking, layout, and the glyph atlas.

**Owns:** ShapedRun, TextLayout, font subsetting hooks, the layout cache.

**Does not own:** Drawing (vela-render); widget layout (vela-ui).

Rank `1`. See `docs/ARCHITECTURE.md §1`.
