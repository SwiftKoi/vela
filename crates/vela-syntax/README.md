# vela-syntax

The lexer and parser: `.vela` source text to a concrete syntax tree, and back again.

**Owns:** Tokens, comments (as trivia), indentation handling, CST node types, parse error recovery,
and the **formatter** (`format`, `print/`) — which is a function of the tree, so it belongs here
rather than next to the command that runs it (`TOOLING.md §3`).

**Does not own:** Name resolution (vela-hir); types (vela-types); diagnostic rendering (vela-diag).

Rank `1`. See `docs/ARCHITECTURE.md §1`.
