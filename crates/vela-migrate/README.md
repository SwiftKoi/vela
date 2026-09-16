# vela-migrate

The Ren'Py `.rpy` to `.vela` transpiler and its compat report.

**Owns:** The `.rpy` reader, the transpilation rules, the project walk, and the report generator.

**Does not own:** Compiling the result (`vela-compile`), or the command line (`vela-cli`) — the
crate plans a migration and writes it, so the rules can be tested without a filesystem.

Rank `8`. See `docs/ARCHITECTURE.md §1` and `docs/spec/TOOLING.md §8`.
