# vela-vm

The bytecode interpreter: a deterministic state machine that yields presentation commands.

**Owns:** Vm, Frame, the step loop, the effect and command registries, and the step-level debug
interface a DAP server drives (`Site`, `FrameInfo`, `Vm::step`; `RUNTIME.md §9`).

**Does not own:** Rendering (vela-render); snapshots and rollback (vela-replay); state
(vela-world); the debugger itself (vela-debug).

Rank `7`. See `docs/ARCHITECTURE.md §1`.
