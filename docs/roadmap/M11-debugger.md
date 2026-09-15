# M11 — Debugger and docs

**Goal.** Debug a story like software, including backwards.

**Depends on.** M10.

**Crates.** `vela-lsp` (DAP module) or a dedicated `vela-debug`.

**Work items.**
1. DAP server over the VM's trace hooks (`RUNTIME.md §9`).
2. Breakpoints by label and by line; step over/in/out across frames.
3. Variable view using debug slot names; `World` inspection.
4. Expression evaluation reusing the real type checker.
5. Time-travel stepping built on rollback snapshots.
6. Trace hooks compiled out of release builds.
7. Generated docs published and linked from the LSP hover text.

**Exit criteria.**
- [ ] VS Code attaches, hits a label breakpoint, steps, and inspects `World`
- [ ] An invalid evaluate expression returns a normal `Exxx`, no crash
- [ ] Stepping backwards over 10 commands yields byte-identical state to the forward run
- [ ] Release builds carry no debug info (`Header::flags` verified in a test)
- [ ] **Demo:** recorded debugging session in the docs

**Risks.** DAP is a large surface for limited scope. Mitigation: implement only the
capabilities listed; refuse others explicitly rather than half-implementing.
