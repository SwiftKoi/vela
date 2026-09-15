# vela-syntax

The lexer and parser: `.vela` source text to a concrete syntax tree.

**Owns:** Tokens, indentation handling, CST node types, parse error recovery.

**Does not own:** Name resolution (vela-hir); types (vela-types); diagnostic rendering (vela-diag).

Rank `1`. See `docs/ARCHITECTURE.md §1`.
