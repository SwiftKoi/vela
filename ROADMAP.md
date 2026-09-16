# Roadmap

How Vela gets built, in what order, and what "done" means at each step. Written to be
executed one work item at a time.

Milestones are ordered by **dependency**, not by importance. Nothing is scheduled by duration;
the ordering is the contract, and each milestone's exit criteria are the gate to the next.

Each milestone lives in its own file under [`docs/roadmap/`](docs/roadmap/) so a working
session can read just the one it is on.

---

## How we work

These rules apply to every milestone. They are not repeated in the milestone files.

1. **One implementation step = one branch = one PR.** A step is usually a whole milestone,
   not an individual work item — work items are commits *inside* it. Sized to the Definition
   of Done in `docs/engineering/CONVENTIONS.md §5`; if a step does not fit, split it. Branch
   naming, when CI runs, and the squash-merge rule are in `docs/engineering/CONVENTIONS.md §6`.
2. **Spec-first.** If implementing something shows the spec is wrong, fix the spec in the
   same PR. `docs/` is the contract; drift is a defect. `docs/README.md` says what lives where
   and how each kind of document is updated — including the milestone skeleton below.
3. **Always runnable.** Every milestone ends with a command a human can execute and observe.
   A milestone that produces only internal code and no observable behavior is mis-scoped —
   fold it into its neighbor.
4. **File budget is a gate, not a target.** `REPO_LAYOUT.md §3` hard limits are enforced by
   `cargo xtask check-file-size`. A work item that needs a file to grow past budget is split
   instead.
5. **Every new diagnostic has a test.** `check-diag-codes` fails otherwise.
6. **Golden drift is reviewed, never auto-accepted.** `cargo xtask bless` regenerates; CI
   fails on unblessed changes.
7. **Registries from day one.** The first time a dispatch has three arms, it becomes a
   registry (`CONVENTIONS.md §1.2`). Retrofitting registries later is how core files reach
   3k lines.
8. **Milestone completion** = all exit criteria checked **and** the demo command demonstrated
   in the PR description.

### Execution note for agent sessions

Start each session by reading the current milestone's file and the crate `README.md`s it
touches. Work items are written to be independently completable. If a work item's files are
not in the crate you expected, that is a signal the architecture has drifted — stop and fix
the boundary rather than working around it.

---

## Milestones

| # | Milestone | Goal in one line |
| --- | --- | --- |
| [M0](docs/roadmap/M00-bootstrap.md) | Bootstrap | A workspace that cannot decay into a monolith, with one runnable binary |
| [M1](docs/roadmap/M01-front-end.md) | Front end | `.vela` source becomes a CST, with precise diagnostics and recovery |
| [M2](docs/roadmap/M02-semantics.md) | Semantics | Names resolve, types agree, story graph analyzed |
| [M3](docs/roadmap/M03-mir.md) | MIR | Checked programs lower to a typed CFG |
| [M4](docs/roadmap/M04-bytecode.md) | Bytecode | Verified bytecode modules in a versioned container |
| [M5](docs/roadmap/M05-vm.md) | VM | A story runs to completion headless, deterministically |
| [M6](docs/roadmap/M06-presentation.md) | Presentation | A window with correct text, from real bytecode |
| [M7](docs/roadmap/M07-screens.md) | Screens | Declarative UI, statically checked, hot-reloadable |
| [M8](docs/roadmap/M08-state.md) | State | Save, rollback, and replay as player-visible features |
| [M9](docs/roadmap/M09-build.md) | Build | Ship to real targets, with small patches |
| [M10](docs/roadmap/M10-tooling.md) | Tooling | LSP, formatter, test runner, and analysis |
| [M11](docs/roadmap/M11-debugger.md) | Debugger | Debug a story like software, including backwards |
| [M12](docs/roadmap/M12-migration.md) | Migration | A Ren'Py project ports incrementally, measurably |
| [M13](docs/roadmap/M13-extensibility.md) | Extensibility | Third parties extend the engine; rich media works |
| [M14](docs/roadmap/M14-hardening.md) | 1.0 | Every claim in `VISION.md §5` is measurably true |

---

## Dependency graph

```
M0 bootstrap
 └─► M1 front end ─► M2 semantics ─► M3 MIR ─► M4 bytecode+verifier ─► M5 VM + headless runtime
                                                                          │
                        ┌─────────────────────────────────────────────────┤
                        ▼                                                 ▼
                   M6 render+text ─► M7 screens                    M8 save/rollback/replay
                        │                    │                          │
                        └────────┬───────────┴──────────────────────────┘
                                 ▼
                          M9 build + assets + web
                                 │
        ┌────────────────────────┼────────────────────────┐
        ▼                        ▼                        ▼
   M10 tooling            M11 debugger+docs          M12 migration
        └────────────────────────┴────────────────────────┘
                                 ▼
                    M13 extensibility + rich media
                                 ▼
                          M14 1.0 hardening
```

**Parallelizable work** (useful with multiple concurrent sessions):
- After M2: M10's LSP diagnostics/hover/completion can begin — it needs the front end, not
  presentation.
- After M5: M8 and M6 are independent.
- After M9: M11 and M12 are independent.
- Throughout: `xtask` checks and the golden corpus grow with whichever milestone lands first.

---

## Post-1.0 backlog

Explicitly **not** in 1.0, in rough priority order:

1. **GUI editor** — the largest item; builds on the LSP and the formatter, edits the same
   files, never introduces a project format.
2. **User-defined generics and traits** — deferred from `LANGUAGE.md §10`; costs more in error
   quality than it returns until proven necessary.
3. **Rich inline text runs** — resolved at M7 with real samples.
4. **RTL and vertical text** — `vela-text` is designed to allow it.
5. **Shared/distributed `vela check` cache** — a CI speedup, not a design dependency.
6. **Additional targets** — consoles, if there is demand and a portability story.
7. **Localization round-trip tooling** beyond coverage reporting.
8. **Multiplayer/shared-reading mode** — interesting, and the deterministic input log makes it
   unusually tractable; still not 1.0.

---

## Risk register

The five ways this project most plausibly fails, and what we do about each.

| Risk | Why it is dangerous | Mitigation |
| --- | --- | --- |
| **Scope creep into a general game engine** | Dilutes everything; never ships | Non-goals in `VISION.md §4`; M14 accepts no new features |
| **Determinism proves leaky late** | Invalidates saves, rollback, and tests simultaneously | Replay equality test exists from M5; cross-arch matrix from M9; linted from M0 |
| **Architecture decays into big files** | The explicit requirement, and the default outcome of any codebase | Hard budget + `xtask` + registries-first rule; enforced on every PR |
| **Ren'Py migration mistranslates** | Silently corrupts someone's project | "Report, never guess" as a tested invariant (M12) |
| **Tooling quality lags the engine** | The differentiator is the whole reason to switch | M10 is a first-class milestone, not a polish phase; parity tests between editor and CLI |

---

## What to do right now

The first five PRs, in order:

1. `M0.1` — workspace skeleton with layer metadata.
2. `M0.2` — `xtask check-layers` + `check-file-size`, wired to CI.
3. `M0.3` — `vela-diag` core: `Diagnostic`, `Code`, `Span`, human renderer.
4. `M0.4` — `vela-cli` command registry + `vela --version`.
5. `M0.5` — `vela new` scaffolding and the `examples/hello` fixture.

After #5, M1 can begin. Do not start M1 before `check-layers` passes on an empty workspace —
the guardrails have to exist before there is anything to guard, or they never get built.
