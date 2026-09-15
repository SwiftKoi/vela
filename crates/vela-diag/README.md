# vela-diag

The diagnostic model: stable codes, spans, severities, and rendering to human, JSON, and SARIF.

**Owns:** The code registry, Diagnostic/Label/Suggestion types, the human renderer.

**Does not own:** Deciding when to emit a diagnostic; each phase owns its own checks.

Rank `1`. See `docs/ARCHITECTURE.md §1`.
