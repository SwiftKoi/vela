# vela-ui

The screen runtime: widget registry, layout solver, styling, and accessibility.

**Owns:** Tree, Widget trait, WidgetRegistry, styles, actions, hot reload. Evaluates a `screen`
into a widget tree (`instantiate`), lays it out, and paints it into a `vela_render::DrawList`
(`paint`) — the bridge that lets the presenter draw a screen rather than a hand-built box.

**Does not own:** Drawing itself (vela-render owns the GPU and the draw list type); text layout
(vela-text shapes and measures). `paint` emits rectangles and glyphs; it does not touch a device.

Rank `8`. See `docs/ARCHITECTURE.md §1`.
