# vela-debug

The debugger: a Debug Adapter Protocol server, so a story can be stepped through a debugger
the way a program can.

**Owns:** The DAP protocol, breakpoints, the step and time-travel policy, variable and
`World` inspection, and expression evaluation for a paused frame.

**Does not own:** The machine it drives (`vela-vm`); the snapshot ring it steps backwards on
(`vela-replay`); the checker it evaluates with (`vela-compile`); the command line (`vela-cli`).

Rank `9`. See `docs/ARCHITECTURE.md §1`, `TOOLING.md §6`, and `RUNTIME.md §9`.
