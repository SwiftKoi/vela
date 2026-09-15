# vela-lsp

The language server: a thin adapter over the vela-compile query database.

**Owns:** LSP protocol handling and capability mapping.

**Does not own:** Analysis; vela-compile owns every query and diagnostic.

Rank `7`. See `docs/ARCHITECTURE.md §1`.
