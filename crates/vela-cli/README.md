# vela-cli

The `vela` binary and its subcommand registry.

**Owns:** Argument dispatch, the Command trait, exit codes, project scaffolding.

**Does not own:** Engine behavior; commands are thin adapters over the engine crates.

Rank `9`. See `docs/ARCHITECTURE.md §1`.
