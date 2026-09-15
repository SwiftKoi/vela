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

**Exit criteria.**
- [ ] `examples/standard` builds and runs on all four targets from one command — it builds; the
      targets and the running are item 5
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

**Progress.** Work items 1 and 3 have landed, along with `E7001` from item 4 and most of item 7.

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
  `tools/wasm-smoke.sh` drives the wasm module in a JavaScript runtime and diffs its command
  stream against `vela run --headless`. Gate 9, with a 300 KB ceiling on the module.

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
- Item 5 (targets) and item 6 (web) — the whole target half, and the two exit criteria that
  cannot be checked here. A real wasm entry point needs an `unsafe` FFI boundary, and the
  workspace sets `unsafe_code = "forbid"`; that is a decision to make deliberately rather than by
  adding an `#[allow]` in passing.
- Sub-file chunks in a patch: a changed file ships whole.

**Verification note.** Two exit criteria cannot be checked on the development machine and are
not covered by CI as configured: *"builds and runs on all four targets"* (there is no macOS
here, and `x86_64-pc-windows-gnu` is not a running Windows) and the browser demo (no
`wasm32-unknown-unknown` target, no bundler, no browser). CI is `ubuntu-latest` only. The asset
half of this milestone is fully checkable; the target half will need either a toolchain
decision or a CI change before its criteria can be ticked honestly.
