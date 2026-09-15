# Build and Asset Specification

Status: **draft, normative for M9.**

This document covers the path from source tree to shippable artifact: how assets are
imported, how the build is made reproducible and incremental, how patches stay small, and
what "one codebase, many targets" actually means.

## 1. Pipeline

```
src/**.vela ──► vela-compile ──► .velac (bytecode + schema + asset refs)
assets/**   ──► vela-assets  ──► imported + transformed artifacts + manifest.json
                                          │
                                          ▼
                                   vela build ──► distribution bundle (per target)
```

The two halves are independent and cacheable. A change to a script does **not** re-import
assets; a change to one asset does not recompile scripts. This split is what makes builds
incremental at all.

> **Implemented so far (M9).** `vela build` runs both halves: every module under `src/` compiles
> to `dist/scripts/<path>.velac`, and every file under `assets/` imports to `dist/manifest.json`
> with its artifacts beside it under `dist/assets/`. `--out <dir>` moves the destination, and a
> project with no `assets/` builds an empty manifest rather than failing — the one-line script
> has to stay buildable.
>
> A build **refuses to ship a story that does not check**. The compiler will assemble a module
> whose checking reported errors, because that is what lets a language server keep working over
> a broken file; the refusal belongs here, at the one command whose job is to produce something
> a player runs. The bundle is verified too — a module that does not verify is an engine bug,
> and saying so beats shipping it.
>
> Debug information is left out of the bundle, as `RUNTIME.md §9` requires of a release build.
>
> **A bundle runs.** `vela run <bundle-dir>` and `vela_vm::Session::load` take a built directory
> straight to the VM: the entry point comes from the manifest, the module from `scripts/`, and no
> `.vela` file is read — so a distribution is something a player starts rather than an artifact
> nothing consumes (`RUNTIME.md §8`, §8 below). `crates/vela-cli/src/tests/bundle_tests.rs`
> observes it by deleting the source tree and running the bundle anyway, and asserting the
> command stream matches the source run.
>
> Targets are item 5 and the launcher is item 8 of `M09-build.md`; per-target layouts, the
> descriptor, the launcher, and what of §4 is live are described in §4 below.

## 2. Asset manifest

Every build produces a content-addressed manifest. It is the single source of truth for what
is in the game.

```jsonc
{
  "manifest_version": 1,
  "name": "forest",                  // the project's name, for a launcher title
  "entry": "main.start",             // where the story starts, `module.label`
  "images": { "bg.forest": "forest.ktx2" },  // image name -> the artifact holding its picture
  "assets": [
    {
      "id": "bg.forest",
      "source": "art/forest.psd",
      "digest": "sha256:…",           // of the *source*
      "artifacts": [                   // one source may yield many outputs
        { "path": "forest.ktx2", "digest": "sha256:…", "kind": "texture",
          "variants": [{ "target": "web", "digest": "sha256:…" }] }
      ],
      "size": { "desktop": 4_210_000, "web": 1_100_000 }
    }
  ]
}
```

Properties:
- **Content-addressed.** `id` is a human name; the `digest` is identity. Two identical inputs
  produce one artifact.
- **Variant-aware.** One source can emit per-target artifacts (e.g. mobile-compressed textures)
  without duplicating the source.
- **The manifest is what the compiler checks `@path` literals against** (`LANGUAGE.md §7.5`),
  so `E7001` and `W7001` come from here.
- **It is also the bundle descriptor.** A built bundle ships no `vela.toml`, so `name`, `entry`,
  and `images` are how it describes itself: where the story starts, what a `scene` names, and
  what a launcher is titled. None is a property of an *import* — an importer never sees a
  project — which is why all three are absent from a bare `import_tree` and omitted from the
  file while they are.

> **Implemented so far (M9).** `vela_assets::Manifest` holds that shape. `size` and `variants`
> exist but are empty until targets do, and are *omitted from the JSON while they are* — so a
> manifest written today is byte-identical to one written after they start being filled in.
>
> The digest is `sha256:`, over the source's bytes for an asset and over the artifact's bytes
> for an artifact. That is a different choice from `vela-replay`'s deliberately
> non-cryptographic schema fingerprint, and for a different reason: "two identical inputs
> produce one artifact" and §6.2's "the patch verifies every chunk digest" both need a hash that
> does not collide, where a fingerprint only needs to notice a change.
>
> An `id` is derived from the source path with the extension dropped and separators made dots —
> `art/forest.png` is `art.forest` — because nothing declares ids yet. When `image`
> declarations and `@"path"` literals meet, the declaration wins and this becomes the fallback;
> the field is in the manifest either way, which is why the manifest is not what changes.
>
> The descriptor fields are filled in by `vela build`, not by the importer: `name` and `entry`
> come from `vela.toml`, and `images` from the project's `image` declarations resolved against
> the manifest the import produced. An import writes none of them, so
> `tests/golden/assets/` pins an import and is unaffected by their existence.

## 3. Importers and transformers

### 3.1 Importers (registry — `CONVENTIONS.md §4.4`)

Importer selection is by **magic bytes first, extension second** (so a `.png` that is
actually a JPEG is handled, or reported clearly).

| Kind | Source formats | Output |
| --- | --- | --- |
| Texture | PNG, JPEG, WebP, PSD (flattened layers optional), Aseprite | `ktx2` (Basis/BC7/ASTC per target) |
| Audio | WAV, FLAC, OGG, MP3 | `ogg`/`opus`, plus a streamed music form |
| Video | MP4, WebM | target codec (AV1/H.264/VP9 policy per platform) |
| Font | TTF, OTF, WOFF2 | subset TTF + metrics sidecar |
| Data | TOML, JSON, CSV | typed value blob, validated against a declared schema |
| Script | `.vela` | bytecode (handled by `vela-compile`, but via the same registry) |

> **Implemented so far (M9).** `ImporterRegistry` selects by magic bytes first and extension
> second, as above, and two importers ship:
>
> * **data** — JSON, TOML, and CSV → canonical JSON, compact with keys sorted, so two files
>   differing only in key order or whitespace are one artifact rather than two. TOML's datetime
>   becomes an RFC 3339 string, because JSON has no such type and the derived conversion encodes
>   it as a struct only `toml` can read. CSV is a table rather than a tree, so it becomes an
>   array of objects with its header row naming the keys — and **every cell is a string**, because
>   a CSV file does not say what a column means and reading `007` as `7` is a silent corruption.
>   The types §3.1 asks for come from a declared schema, which does not exist yet.
> * **texture** — PNG, validated by its magic bytes and re-encoded with the colour type and bit
>   depth it already had and nothing else. The ancillary chunks do not survive, so two exports
>   of one image that differ only in the metadata a tool wrote into them import to the same
>   artifact. **Not transcoded**: §3.1 asks for `ktx2`, which needs a Basis/BC7/ASTC transcoder
>   this build does not have, and a file named `.ktx2` holding PNG bytes would pass every check
>   in this repository and fail on a player's GPU.
>
> Audio, video, fonts, CSV, and the script importer are not written yet. Until they are, a file
> nothing claims **fails the build** rather than being skipped: an asset silently dropped from a
> build is a blank rectangle in front of a playtester, and §9's `E7003` exists precisely so it is
> not.

### 3.2 Transformers
Transformers run between import and pack: compression, atlas packing, mip generation,
trim, color-space conversion, audio normalization, and font subsetting.

**Font subsetting is a first-class feature, not an optimization.** The subset is computed from
the union of (a) all string literals in the script, (b) the translation catalogs for enabled
locales, and (c) a safety set for user-entered text and runtime-formatted numbers. A CJK game
that would otherwise ship 15 MB of fonts per locale ships a fraction of it — and adding a
locale re-subsets automatically.

### 3.3 Determinism of imports
Importers are pure functions of source bytes plus declared options. Parallel import is allowed
**only** because the result is order-independent (content-addressed). An importer that
produces different bytes for the same input is a bug (`check-determinism` covers the
in-process rules; the seed corpus covers the rest).

## 4. Targets

One command, one source tree:

```
vela build --target web,win,mac,linux,android
```

| Target | Backend | Notes |
| --- | --- | --- |
| `win` | DX12 via `wgpu` | Steam-friendly; code signing hook |
| `mac` | Metal via `wgpu` | Universal binary; notarization hook |
| `linux` | Vulkan | AppImage / Flatpak output options |
| `web` | WebGPU / WebGL2 fallback | `wasm32-unknown-unknown`; see §5 |
| `android` | Vulkan/GLES | APK/AAB; touch input profile |
| `ios` | Metal | Notarized; App Store packaging hook |

Target selection changes: (a) which artifact variants are packed, (b) the shader backend,
(c) the input profile defaults, and (d) the signing/packaging step. It does **not** change
game logic — the VM, World, and bytecode are target-independent by construction, which is
what the layer rules in `REPO_LAYOUT.md §1` protect.

> **Implemented so far (M9).** `vela build --target win,mac,linux,web` writes one self-contained
> bundle per target — `dist/linux/`, `dist/web/`, … — each with the same scripts and assets and
> its own `target.json` descriptor and **launcher**. The script tree is identical across targets
> *because the story is target-independent by construction*; what differs is the descriptor and
> the launcher, and those are real files a person and a runtime read.
>
> What of the four dimensions §4 names is live, stated rather than implied:
>
> * **(a) variants** — **not implemented.** `target.json` records an empty variant set, because
>   there is nothing to record: no importer emits a variant (§3.1's `ktx2`/`ogg` need
>   transcoders this build does not have), and a `Variant` carries a digest with no path beside
>   it, so a selector could not name a file to pack even if one existed. Every target packs the
>   default artifact.
> * **(b) backend** — recorded (`vulkan`, `metal`, `dx12`, `webgpu-webgl2`). The renderer does
>   not choose a backend from it yet.
> * **(c) input profile** — recorded *and consumed*: `vela run <bundle>` reads it and installs
>   the bindings (`vela-host::Bindings::profile`). All four targets shipped here are
>   pointer-and-keyboard and share one profile; a touch profile arrives with `android`.
> * **(d) packaging** — a declared hook, never executed by the build: signing wants credentials,
>   and §8 keeps those out of the project tree.
>
> A flag that produced the same bytes for every target would be cosmetic. This one does not —
> but the honest account is that it is not cosmetic because of the *launcher and descriptor*, not
> because variants or backends are being selected yet.

## 5. Web specifics

Web is a first-class target, not a port (`VISION.md §3.4`).

- **Rendering**: WebGPU where available, WebGL2 fallback for older browsers. The render graph
  (`vela-render`) is expressed so it can lower to either.
- **Threads**: single-threaded by default so no COOP/COEP headers are required — deploying to
  itch.io must not require custom headers. A threaded build is opt-in for projects that can
  control headers.
- **Assets**: streamed via range requests; the manifest enables loading only what the current
  scene needs. `vela build --web` emits a prefetch plan derived from the story graph — the
  assets reachable from the current label are fetched ahead of the player.
- **Saves**: `IndexedDB` via the `fs` capability. Saves are exportable/importable as a file so
  a player is never trapped in a browser profile.
- **Size**: the wasm engine core is size-gated in CI (a budget that must be *lowered*, never
  raised, without an explicit decision) so the web target stays a viable instant demo.

> **Implemented so far (M9).** The precondition holds and is *checked*, and the engine now runs
> there. `crates/vela-web` is the browser entry point: a `Player` a page constructs from a
> `.velac` and drives with `step` / `line` / `choose`. `tools/wasm-smoke.sh` builds it, drives the
> **web bundle** (`--target web`) in a JavaScript runtime — entry point and module from the
> manifest, as a page would — and **diffs what it presents against `vela run --headless`**: one
> engine, two platforms, one command stream, which is what makes the determinism contract survive
> the platform boundary rather than stopping at it.
>
> It is a `cdylib` behind `wasm-bindgen`, so no `unsafe` had to be added to this workspace: the
> raw pointers a wasm ABI is made of live in that dependency, and `unsafe_code = "forbid"` stays
> as written. The crate is `wasm32`-only — on a native target it compiles to an empty library,
> because a browser entry point has no meaning off a browser.
>
> `--target web` also lays the bundle out with a `target.json` and an `index.html` **launcher**
> that fetches the manifest, the module, and the engine glue, then plays. The glue
> (`vela_web.js`) is a separate artifact — a browser target's engine is not its assets — and is
> produced by the wasm build rather than by `vela build`, which has no toolchain in it.
>
> The module is 287 KB and size-gated in CI (gate 9), against a ceiling that is meant to be
> **lowered** and raised only with a reason.
>
> **Still to come: drawing in the browser.** A bundle that plays in a browser *window* needs the
> renderer on `wgpu`'s web backends and a canvas, and `--serve` to look at it. Until then the
> `index.html` launcher plays in text — a real engine rather than a demonstration of one, but not
> yet something a player would recognise as a game.

## 6. Incremental builds and delta patches

### 6.1 Incremental
The build is a query graph: `import(asset)`, `transform(asset)`, `compile(script)`, `pack(target)`.
Only changed inputs and their dependents re-run. A typical art tweak re-imports one texture;
a script typo recompiles one module.

### 6.2 Delta patches
Because every artifact is content-addressed, a patch between releases is computed as a set
difference plus binary diffs of changed chunks:

```
vela build --release --patch-from ./dist/1.4.0 --patch-out ./dist/1.4.1-patch
```

- The patch contains only added and changed chunks, plus a small manifest diff.
- Applying a patch verifies every chunk digest before writing, so a corrupted download is
  detected and the full build is fetched instead.
- **Acceptance bar** (`VISION.md §5`): a text-only change must produce a patch under 5% of the
  full bundle size. This is a CI benchmark, enforced on a fixture project.

> **Implemented so far (M9).** `vela build --patch-from <previous> --patch-out <dir>` computes
> the difference between two bundles and writes `patch.json` plus the changed blobs under
> `blobs/`, each named by its own digest so two changed files with the same content cost one.
> `vela patch apply <patch> <bundle>` puts them on top of the build the patch was computed from.
>
> Applying verifies **before** it writes: the bundle's identity against the base the patch was
> computed from, and every blob against the digest the index records. That ordering is the
> whole promise — a half-applied bundle is neither version, and "fetch the full build instead"
> is only an option while the old one is still there.
>
> The acceptance bar holds and is a test: a one-line text change to a bundle of three 200 KB
> assets produces a patch of a few hundred bytes, which is the changed script and nothing else.
> It runs under `cargo test`, so it is gate 5 rather than a gate of its own.
>
> **Not yet: sub-file chunks.** A changed file ships whole, so a one-byte edit to a large
> texture costs the texture. The bar is met at this granularity because a text change alters a
> script and nothing else; diffing file *contents* is what the next level of this needs. The
> format has room for it — an entry names a path and a digest, and a chunked entry would name
> several — which is why the version is in the index.

## 7. Reproducibility

Requirement: building the same source twice produces byte-identical artifacts.

Enabled by: pinned toolchain (`rust-toolchain.toml`), no embedded timestamps or absolute paths
in artifacts, sorted manifest entries, deterministic compression settings, and
content-addressed packing. `vela build --verify-reproducible` builds twice and diffs — run in
CI on the fixture project. A reproducible build is what makes a delta patch meaningful and a
release auditable.

> **Implemented so far (M9).** `vela build --verify-reproducible` builds the project a second
> time into a directory of its own — a fresh session and a fresh import, not a copy of the
> first — and compares the two trees file by file, so a failure names what differed instead of
> saying the bundles are unequal. It is gate 6 in CI (`REPO_LAYOUT.md §6`), run on the standard
> example.
>
> What makes it hold: no timestamps and no absolute paths anywhere, sources relative with `/`
> separators, assets sorted by id and artifacts by path, and a content-addressed codec. What is
> **not** yet held: nothing refuses an importer that is not pure — §3.3 is a rule the registry
> documents rather than one it enforces — and the check compares two builds *in one
> environment*, so it cannot see a difference that would only appear on another machine. The
> cross-machine half is what the pinned toolchain and the release matrix are for.

## 8. Packaging and signing

- `vela build` produces a **distribution bundle** (assets + bytecode + launcher), not a
  platform installer. Installer generation is a thin hook so platform-specific packagers can
  be swapped in without touching the engine.
- Signing/notarization are declared hooks invoked with the path to the produced binary; no
  credentials ever enter the project tree.
- Store integrations (Steam, itch.io) are packaging hooks plus a metadata file. They are not
  built into the engine — this keeps us out of the store-specific-code business.

> **Implemented so far (M9).** `vela build --target <t>` writes a launcher beside each target's
> bundle: `launch.sh` on `mac` and `linux`, `launch.cmd` on `win`, `index.html` on `web`. A
> desktop launcher runs the engine on the bundle directory, preferring a runtime copied beside
> it and falling back to `vela` on `PATH` — the release step that embeds the engine is the thin
> hook this section describes, and it is not done here. The web launcher is a page, and needs
> the wasm glue built beside it (§5).
>
> A target's `target.json` records its backend, input profile, and packaging hook. The hook is
> **not** executed: it names what a release step would run (`codesign`, `notarize`, `appimage`),
> and running it would want credentials, which §8 keeps out of the project tree.
>
> What a launcher starts is a bundle with no compiler in it: `vela run <bundle>` and
> `vela_vm::Session::load` read the manifest's entry point and the module the build wrote. A
> windowed bundle run uses the presenter's built-in dialogue and menu, because screens are
> compiled from source and a bundle ships none — backgrounds do show, from the manifest's
> `images` mapping. Packing compiled screens into a bundle is the next step.

## 9. Diagnostics

| Code | Meaning |
| --- | --- |
| `E7001` | Asset referenced by `@path` does not exist |
| `E7002` | Manifest digest mismatch (corrupted or stale) |
| `E7003` | Importer failed (with the source path and format) |
| `W7001` | Unused asset |
| `W7002` | Asset exceeds its declared size budget |
| `W7003` | Font subset is missing a glyph reachable from interpolated text |
| `E7101` | Bytecode format too new for this engine |
| `E7102` | Bytecode uses a command variant this engine does not know |
| `E7201` | Save is older than the oldest available migration |
| `E7202` | Save is from a newer engine version |
| `E7301` | Attempt to demote an error diagnostic to a warning |

> **Implemented so far (M9).** `E7001` is both specified here and *produced*: `vela check`
> reports every `@"path"` that names nothing in the manifest, at the literal's span
> (`LANGUAGE.md §7.5`). The manifest it checks against is imported on the spot rather than read
> from `dist/`, so the answer is about the assets as they are.
>
> The rest of this table is specified but not yet registered in `codes.txt`, because nothing
> produces it yet and `check-diag-codes` requires a code to be exercised by a test before it can
> be committed. What each is waiting on: `E7002` and `W7002` need targets and size budgets;
> `E7003` is currently an import failure that fails the build with a message rather than a
> rendered diagnostic; `W7001` needs a span, which the manifest does not have until it is a file
> the session knows about; `W7003` needs the font subsetter.

## 10. Open questions

1. **Atlas strategy** (M9): one global UI atlas vs. per-screen atlases. Start per-screen for
   predictable load sizes; revisit with real projects.
2. **PSD layer extraction** (M9): whether to support named-layer import for character sprites
   or require a pre-exported layer stack. The latter is simpler; the former is what artists
   actually have. Decide with an artist workflow test.
3. **Patch compression** (M9): zstd chunk framing vs. a streaming binary diff. Benchmark both
   on the fixture project before committing.
4. **Video codec policy** (M13): per-platform AV1 availability is still uneven; policy will be
   a target matrix with a documented fallback, reviewed each release.
