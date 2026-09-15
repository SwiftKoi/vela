# vela-vm

The bytecode interpreter: a deterministic state machine that yields presentation commands.

**Owns:** Vm, Frame, the step loop, the effect and command registries.

**Does not own:** Rendering (vela-render); snapshots (vela-replay); state (vela-world).

Rank `6`. See `docs/ARCHITECTURE.md §1`.
