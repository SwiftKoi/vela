# M9 — Build, assets, and web

**Goal.** Ship to real targets, with small patches.

**Depends on.** M6, M7, M8.

**Crates.** `vela-assets`, `vela-cli` build/pack commands.

**Work items.**
1. Importer registry + the texture/audio/font/data importers (`BUILD_AND_ASSETS.md §3.1`).
2. Transformer pipeline; **font subsetting** from scripts + translations.
3. Content-addressed manifest, variant-aware (`BUILD_AND_ASSETS.md §2`).
4. `@path` resolution wired to the manifest (`E7001`, `W7001`).
5. Target drivers: `win`, `mac`, `linux`, `web`; packaging hooks for signing/stores.
6. Web specifics: WebGPU + WebGL2 fallback, single-threaded default, range-request streaming,
   graph-derived prefetch plan, `IndexedDB` saves (`BUILD_AND_ASSETS.md §5`).
7. Delta patches + the reproducibility check + the wasm size gate.
8. The bundle runtime: a loader that takes a built bundle straight to the VM
   (`Session::load`), `vela run <bundle>` with no compiler in the path, and the per-target
   **launcher** a player starts (`BUILD_AND_ASSETS.md §8`).

Item 8 was implied by §8's "assets + bytecode + launcher" but was not a work item of its own —
item 5 reads as platform plumbing, so the launcher went missing and every run recompiled from
source. Naming it is the point: `vela build` already produced a bundle nothing consumed.

**Exit criteria.**
- [ ] `examples/standard` builds and runs on all four targets from one command — the build is
      item 5 and the running is items 5 and 8; see the progress note for what is verified where
- [x] Reproducible build: two builds are byte-identical
- [x] A text-only change produces a patch < 5% of the full bundle — a test in `cargo test`, so
      gate 5 rather than a gate of its own
- [x] Adding an importer touches only its own file + the registry (`CONVENTIONS.md §4.4`)
- [x] wasm core size is at or under its gate — 287 KB, gated in CI behind a ceiling meant to be
      lowered
- [ ] **Demo:** `vela build --target web --serve` and play the fixture in a browser — the wasm
      engine runs and plays the same story as the native build (`tools/wasm-smoke.sh`); what is
      missing is `--target web` laying the bundle out, `--serve`, and drawing it

**Risks.** Target-specific code leaking into core is the main threat to the layer rules.
Mitigation: `check-layers`'s adapter rule runs on every target driver PR.

---

**Progress.** Work items 1, 3, 5, and 8 have landed, along with `E7001` from item 4 and most of
item 7.

- `Manifest`, `Asset`, `Artifact`, `Variant` — content-addressed, canonically ordered, with
  `size` and `variants` present but empty (and omitted from the JSON while they are, so a
  manifest written today is byte-identical to one written after targets exist).
- `ImporterRegistry`, selecting by magic bytes first and extension second, plus the **data**
  importer (JSON and TOML → canonical JSON) and a **texture** one (PNG, validated by
  magic bytes and re-encoded with no ancillary chunks).
- `vela build [path] [--out <dir>]` → `dist/scripts/**/*.velac` and `dist/manifest.json` with
  `dist/assets/**`. Both halves of §1's pipeline, in one command.
- A build **refuses to ship a story that does not check**, so the editor and the build agree.
- `vela check` reports `E7001` for a `@"path"` that names nothing in the manifest.
- `--verify-reproducible` builds twice and compares; it is gate 6 in CI (`REPO_LAYOUT.md §6`).
- `--patch-from <previous> --patch-out <dir>` and `vela patch apply` — content-addressed delta
  patches that verify before they write. A one-line text change to a 600 KB bundle is a few
  hundred bytes.
- `tests/golden/assets/` pins the whole import as a golden.
- The compiler, the VM, the save layer, and the asset layer compile for
  `wasm32-unknown-unknown` with no source changes, and `crates/vela-web` runs the engine there:
  `tools/wasm-smoke.sh` drives the **web bundle** in a JavaScript runtime and diffs its command
  stream against `vela run --headless`. Gate 9, with a 300 KB ceiling on the module.
- Item 8, the bundle runtime: `vela_vm::Session::load` takes a built bundle straight to the VM —
  entry point from the manifest, module from `scripts/`, no compiler — and `vela run <bundle>`
  runs one. `crates/vela-cli/src/tests/bundle_tests.rs` deletes the source tree after building
  and asserts the bundle still plays the same command stream, which is what makes "loads without
  recompilation" an observation rather than a claim.
- Item 5, the target drivers: `vela build --target win,mac,linux,web` writes one self-contained
  bundle per target with its own `target.json` descriptor and launcher. What §4's four dimensions
  actually do today — variants **not implemented**, backend recorded, input profile *consumed*,
  packaging a declared hook — is written down in `BUILD_AND_ASSETS.md §4` rather than implied.
- The launcher per target: `launch.sh` / `launch.cmd` / `index.html`. A windowed bundle run uses
  the built-in presenter and the manifest's images; packed screens are the next step.

Not yet, and in the order they are needed:

- The rest of item 1: audio, video, fonts, and the script-via-the-registry importer. A file
  nothing claims currently **fails** the build rather than being skipped. The spec's outputs for
  the first three are `ktx2` and `ogg`/`opus`, which need transcoders this build does not have;
  what is reachable first is validation and magic-byte selection, which is what the texture
  importer does.
- Item 2: transformers and font subsetting. Subsetting is where the first real **decision** is:
  doing it correctly means `subsetter`/`skrifa`/`write-fonts` — nine crates — and doing it by hand
  means writing a TTF rebuilder, which is a project of its own and the one place a subtle bug
  ships broken text to a player. Not taken unilaterally.
- Item 4's other half: `W7001` (unused asset) has no honest span — the manifest is not a source
  file the session knows about, and a diagnostic must point somewhere. It needs either a
  span-less diagnostic in `vela-diag` or the manifest as a known source file; both are model
  changes, not a small addition.
- Item 5's two live-but-unfed dimensions: **variant packing** needs an importer that emits a
  variant (the transcoders §3.1 asks for), and **backend selection** needs a renderer that chooses
  a backend from the descriptor. The driver, the descriptor, and the launcher are here; the inputs
  each would act on are not.
- Item 6 (web specifics): the graph-derived prefetch plan, range-request streaming, `IndexedDB`
  saves, and the WebGL2 fallback. The browser **demo** (exit criterion 6) stays open for the
  reason its note gives: it needs the renderer on `wgpu`'s web backends and a canvas.
- Packing compiled screens into a bundle: screens are compiled from source, so a windowed bundle
  run uses the built-in presenter. Backgrounds show — the manifest records the `images` mapping —
  but a project's own `dialogue` and `pause` screens are not in the bundle yet.
- Sub-file chunks in a patch: a changed file ships whole.

**Verification note.** Exit criterion 1 — *"builds and runs on all four targets from one
command"* — cannot be checked on the development machine: there is no macOS here, and
`x86_64-pc-windows-gnu` is not a running Windows. Until this work it was covered by no CI job
either, which is how a bundle that only *built* passed for a bundle that *ran*. It is now covered
by two: the `platforms` matrix builds each native target's bundle with `--target` and **runs it
from `dist/`**, diffing the command stream against a source run; gate 9 plays the web bundle in
wasm. The criterion is therefore ticked by those jobs going green, not by prose here — and it was
*not* ticked on the machine this was written on.

The browser demo (exit criterion 6) is still open and still not checkable here: it needs the
renderer on `wgpu`'s web backends and a canvas, and there is no `wasm32-unknown-unknown` target,
bundler, or browser on the development machine. CI has no browser either; gate 9 runs the engine
in Node, which exercises the command boundary rather than the canvas.
