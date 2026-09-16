# M13 — Extensibility and rich media

**Goal.** Third parties extend the engine; the media a VN actually needs works.

**Depends on.** M9, M11.

**Crates.** `vela-plugin`, `vela-audio`, `vela-ui`/`vela-render` media widgets.

## Work items
1. WASM plugin host (wasmtime) with the capability ABI and version negotiation
   (`ARCHITECTURE.md §6.4`); `check-abi`.
2. Plugin registration into every registry: effects, widgets, importers, passes, CLI commands.
3. Audio: graph description, buses, music/voice/SFX channels, streaming, position reporting for
   save/restore.
4. Animation/transitions polish: springs, composed transforms, transition library
   (`SCREENS.md §6`).
5. Rich media widgets: `video`, `live2d`, `particles`, and 3D-stage integration
   (`SCREENS.md §13`).
6. Accessibility pass across the whole widget set; contrast and focus audits.
7. Mobile targets: `android`, `ios`, touch input profile, safe-area handling, lifecycle
   (backgrounding mid-animation must replay correctly).

## Exit criteria
- [ ] A sample plugin adds a widget, an effect, and a lint touching **zero** core files
- [ ] A plugin requesting an undeclared capability is denied deterministically
- [ ] Audio position survives save/load exactly
- [ ] Backgrounding during an animation and resuming replays to the same state
- [ ] The full widget set passes the accessibility sweep
- [ ] **Demo:** `vela run` a fixture using the sample plugin, Live2D, and audio

## Status

Not started. Nothing here is designed yet beyond the ABI shape `ARCHITECTURE.md §6.4` fixes.

## Risks

The plugin ABI is the hardest thing here to get right and the most expensive to
change. Mitigation: ship ABI 1.0 with the smallest possible surface — effects and widgets
only — and add surfaces additively.
