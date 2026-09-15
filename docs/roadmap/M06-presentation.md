# M6 — Presentation core: renderer and text

**Goal.** A window with correct text, from real bytecode.

**Depends on.** M5.

**Crates.** `vela-render`, `vela-text`, `vela-host` (window/input backends).

**Work items.**
1. `wgpu` device/swapchain setup; a render graph with insertable stages (`ARCHITECTURE.md §5`).
2. `vela-text`: shaping, line breaking, bidi-ready layout, glyph atlas, change-driven cache
   invalidation (`ARCHITECTURE.md §8`).
3. A `Command` consumer that turns `Say`/`Scene`/`Show`/`Hide` into draw lists.
4. `Host` trait + the native backend: window, input → semantic actions (`SCREENS.md §11`).
5. Shader backend selection per target (DX12/Metal/Vulkan).
6. Frame-latency and startup benchmarks in CI (budgets, so regressions are visible).

**Exit criteria.**
- [x] `examples/hello` shows dialogue in a window and advances on input
- [x] Text layout golden: a fixed string lays out byte-identically across the CI matrix
- [x] Startup budget met on the benchmark fixture
- [x] No core file exceeds budget; new render stages were added without touching the graph core
- [x] **Demo:** `vela run examples/hello`

**Risks.** Text rendering is a deep rabbit hole. Mitigation: scope to what a VN needs —
shaping, wrapping, emphasis; rich inline runs are deferred and gated on real samples at M7.

---

**Status.** Complete. 285 tests, 7/7 `xtask` checks, clippy clean, budgets measured.

## What the milestone actually cost

Almost all of it was spent on one question — *can this be verified without a display* — and
the answer shaped the design rather than being worked around.

Scraping X does not work: a Vulkan swapchain presents through DRI3/Present and never reaches
Xvfb's framebuffer, so `import -window root` captures black against a window that is running
and presenting. The renderer therefore draws into a *target*, and a window is one target
among others. That is what the exit criterion requires — "byte-identically across the CI
matrix" — because CI has no display, and a golden that only runs on a developer's machine is
not a golden.

The same question, one level down, produced the harness: `tools/drive.sh` runs the app under
`xvfb-run`, sends real XTEST input through `xdotool`, and reads the *story's own output* to
tell whether a keypress landed, since nobody can see the window.

## The bug worth remembering

Every rectangle rendered as a **bowtie** — two triangles sharing the left edge instead of the
anti-diagonal, leaving the right wedge uncovered. I dumped the vertices and indices, they were
exactly what I had written, and I nearly moved on: the data was "correct" in the sense that it
matched my intent.

It was correct and wrong. **I verified the code matched my intent instead of verifying the
intent was right.** What found it was printing the frame as an ASCII map of exact pixels,
where a bowtie is numbered rows rather than a shape someone has to squint at.

## The environment bug worth remembering more

A window kept appearing on the operator's desktop during automated runs while the app
faithfully reported `display=:99`. The cause was `WAYLAND_DISPLAY` in the environment:
`winit` prefers Wayland when it is set and **ignores `DISPLAY` entirely**, so the app was
never an X11 client, the sandbox was irrelevant, and `xdotool` could never deliver a key.

One cause, four symptoms that all looked like separate bugs. The evidence was in my own
diagnostic output — a side-by-side environment dump — and I read it for `DISPLAY`, found
`:0`, and never asked what else was in there.

Worse than the miss: twice in that hunt I presented a *broken check* as a finding — an
`xdotool search --display` flag that does not exist, and a reachability test that read
"found no windows" as "cannot connect". Both failed for the wrong reason and both read as
evidence. The lesson is already in the roadmap in another form; this is what it looks like
when it costs a day.
