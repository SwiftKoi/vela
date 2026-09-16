# M0 — Bootstrap and guardrails

**Goal.** A workspace that cannot decay into a monolith, with one runnable binary.

**Depends on.** Nothing.

**Crates.** Workspace skeleton, `xtask`, `vela-cli` (stub), `vela-span`, `vela-diag` (skeleton).

## Work items
1. Workspace `Cargo.toml` with all crates from `ARCHITECTURE.md §2` as empty libs; each
   declares `[package.metadata.vela] rank` (and `adapter` where applicable).
2. `xtask` with `check-layers` and `check-file-size`; wire into CI.
3. `vela-diag`: `Diagnostic`, `Code`, `Span`; the code registry; human renderer.
4. `vela-cli` stub with the `Command` registry and `vela --version`.
5. `vela new` scaffolding (`examples/hello`), with `vela.toml` schema v1.
6. `check-registries` and `check-exemptions`; `check-determinism` as a path-aware source
   scan (`REPO_LAYOUT.md §4.2`).
7. CI workflow running the gate order in `REPO_LAYOUT.md §6`.

## Exit criteria
- [x] `cargo xtask check-layers check-file-size check-facade` passes on the empty workspace
- [x] A deliberately-upward dependency fails `check-layers` with an actionable message
- [x] A deliberately-oversized file fails `check-file-size` and names the split recipe
- [x] Diagnostics render with code + span + suggestion
- [x] **Demo:** `vela new demo && cd demo && vela --version`

## Status

Complete. 22 crates scaffolded with ranks, 7 xtask checks green, clippy clean
with `-D warnings`, 30 tests passing. `cargo run -p vela-diag --example render_demo` shows
the rendered diagnostic.

## Found during implementation

The 4-layer model could not express the front-end chain
(`span → syntax → hir → types → mir → bytecode`) without allowing same-rank edges, which are
the ones that grow into cycles. Replaced with an integer **rank** per crate; `ARCHITECTURE.md
§1` now states the two rules explicitly. This is the spec-first rule doing its job.

## Risks

Layer metadata is easy to get subtly wrong (e.g. `vela-diag` depending on
`vela-syntax`). Mitigation: the empty-workspace check must pass *before* any real code lands.
Resolved — `check-layers` has been green since the first commit of real code.
