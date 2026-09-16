# M13 — Extensibility and rich media

**Goal.** Third parties extend the engine; the media a VN actually needs works.

**Depends on.** M9, M11, and the M12 series (M12.1–M12.3 come first: a project's screens, the
interface a game ships, and the media it plays, before the plugin host that extends them).

**Crates.** `vela-plugin`, `vela-ui`/`vela-render` media widgets. (`vela-audio` moved to M12.3 with
the audio a game actually plays; what is left here is the audio *engine*.)

## Work items
1. WASM plugin host (wasmtime) with the capability ABI and version negotiation
   (`ARCHITECTURE.md §6.4`); `check-abi`.
2. Plugin registration into every registry: effects, widgets, importers, passes, CLI commands.
3. The audio **engine**, on top of M12.3's playback: a graph description, buses and sends, streaming
   for long sources, and DSP passes. Channels, looping, fading, muting and position-across-save are
   M12.3's and are assumed here.
4. Animation/transitions polish: springs, composed transforms, transition library
   (`SCREENS.md §6`).
5. Rich media widgets: `video`, `live2d`, `particles`, and 3D-stage integration
   (`SCREENS.md §13`).
6. Accessibility pass across the whole widget set; contrast and focus audits.
7. Mobile targets: `android`, `ios`, safe-area handling, lifecycle (backgrounding mid-animation must
   replay correctly). The touch *input profile* is M12.3's, since it is a binding over the same
   semantic actions; the platform work is here.

## Exit criteria
- [ ] A sample plugin adds a widget, an effect, and a lint touching **zero** core files
- [ ] A plugin requesting an undeclared capability is denied deterministically
- [ ] Backgrounding during an animation and resuming replays to the same state
- [ ] The full widget set passes the accessibility sweep
- [ ] An audio bus with a send changes what is heard, and the graph survives save/load
- [ ] **Demo:** `vela run` a fixture using the sample plugin, Live2D, and a routed audio graph

## Status

Not started. Nothing here is designed yet beyond the ABI shape `ARCHITECTURE.md §6.4` fixes.

## Risks

The plugin ABI is the hardest thing here to get right and the most expensive to
change. Mitigation: ship ABI 1.0 with the smallest possible surface — effects and widgets
only — and add surfaces additively.
