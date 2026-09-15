# M7 — Screens and UI

**Goal.** Declarative UI, statically checked, hot-reloadable.

**Depends on.** M6.

**Crates.** `vela-ui`.

**Work items.**
1. Screen compile: `screen` decl → widget tree + static dependency sets (`SCREENS.md §8.2`).
2. Layout solver: measure/arrange, containers `box`/`row`/`column`/`grid`/`flow`/`stack`.
3. Widget registry + the default widget set, **one file per widget**.
4. Style/theme resolution and token validation (`E5007`, `E5008`), contrast lint `W4009`.
5. Actions as a registry; the built-in set from `SCREENS.md §7`.
6. Reactivity via `World` change log; assert full-tree relayout does **not** happen per frame.
7. Accessibility tree, self-voicing, focus order; `W4010` for unlabeled interactive nodes.
8. Hot reload: tree diff with id-based state preservation (`SCREENS.md §12`).
9. Screen diagnostics `E5004`–`E5008`, `W4007`, `W4008`.

**Exit criteria.**
- [ ] A dialogue box, a menu, a settings screen, and a save/load screen all written in Vela
- [ ] Unknown widget / wrong prop / bad arg each produce the right `E5xxx` with a suggestion
- [ ] A benchmark proves a static screen does not relayout when unrelated state changes
- [ ] Editing a screen during `vela run` preserves scroll position and input buffers
- [ ] Every screen passes the `--a11y` focus-order sweep
- [ ] **Demo:** `vela run examples/standard` — menus, settings, keyboard and gamepad nav

**Risks.** Layout systems accrete escape hatches until they are absolute positioning again.
Mitigation: `W4007` lints `absolute` usage so the pressure to fix the layout model is visible.
