# M7 — Screens and UI

**Goal.** Declarative UI, statically checked, hot-reloadable.

**Depends on.** M6.

**Crates.** `vela-ui`.

## Work items
1. Screen compile: `screen` decl → widget tree + static dependency sets (`SCREENS.md §8.2`).
2. Layout solver: measure/arrange, containers `box`/`row`/`column`/`grid`/`flow`/`stack`.
3. Widget registry + the default widget set.
4. Style/theme resolution and token validation (`E5007`, `E5008`), contrast lint `W4009`.
5. Actions as a registry; the built-in set from `SCREENS.md §7`.
6. Reactivity via `World` change log; assert full-tree relayout does **not** happen per frame.
7. Accessibility tree, self-voicing, focus order; `W4010` for unlabeled interactive nodes.
8. Hot reload: tree diff with id-based state preservation (`SCREENS.md §12`).
9. Screen diagnostics `E5004`–`E5008`, `W4007`, `W4008`.

Item 3 said "one file per widget" and reality chose otherwise. The built-in widgets are entries in
`widgets/builtin.rs`'s `ALL`, because a widget *is* a declaration — a name, a category, and a prop
schema — and thirteen files holding one struct literal each would be thirteen places to look for
what is one table. The rule the item was reaching for is the one that held: adding a widget touches
one file and nothing else (`CONVENTIONS.md §4.2`).

## Exit criteria
- [x] A dialogue box, a menu, a settings screen, and a save/load screen all written in Vela —
      `examples/standard`: `dialogue`, `pause` (Save, Load, Settings, Credits), `settings`, `credits`,
      and a `menu` in `chapters/street.vela`
- [x] Unknown widget / wrong prop / bad arg each produce the right `E5xxx` with a suggestion —
      `check-diag-codes` requires a test per code, and `E5004`–`E5008` have theirs
- [x] A benchmark proves a static screen does not relayout when unrelated state changes —
      `crates/vela-ui/tests/reactivity.rs::a_static_screen_never_relays_out`
- [x] Editing a screen during `vela run` preserves scroll position and input buffers —
      `crates/vela-ui/src/reload.rs` diffs two trees by `id`, and
      `tests/reload.rs::editing_a_prop_keeps_everything` is the case the criterion names: a prop is
      nudged and the input box keeps its cursor
- [x] Every screen passes the `--a11y` focus-order sweep — `vela test --a11y`, asserted in
      `crates/vela-cli/src/tests/ui_tests.rs` (passing screen, and one that must fail)
- [x] **Demo:** `vela run examples/standard` — menus, settings, keyboard navigation

## Status

Complete. `vela-ui` owns the screen runtime end to end: a `screen` declaration compiles to a widget
tree with its read-set, the solver measures and arranges it, styles and theme tokens resolve against
the schema, actions are a registry, focus order follows tree order, the accessibility tree comes from
the same walk, and a reload decides what carries across by identity. `examples/standard` is the
proof, and it is the project the CI gates run.

## Still open

- **Gamepad and touch input.** The input model is semantic actions (`SCREENS.md §11`) and one host
  mapping — a keyboard — is written. The abstraction is the part that was asked for here; the other
  profiles arrive with the targets that need them (M13's mobile work item).
- **Transforms and rich text.** `at` and the transform grammar are M13's, and rich inline text runs
  were deferred to real samples (`ROADMAP.md` backlog). `SCREENS.md §4` records both.

## Risks

Layout systems accrete escape hatches until they are absolute positioning again.
Mitigation: `W4007` lints `absolute` usage so the pressure to fix the layout model is visible.
