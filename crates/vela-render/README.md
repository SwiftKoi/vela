# vela-render

Adapter: the wgpu renderer, render graph, and per-target shader backends.

**Owns:** Renderer, RenderGraph, draw lists, shader backend selection.

**Does not own:** Text layout (vela-text); widget trees (vela-ui).

Rank `2`, **adapter**. See `docs/ARCHITECTURE.md §1` rules 1 and 2.
