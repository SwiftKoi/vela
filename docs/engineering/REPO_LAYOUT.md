# Repository Layout

## 1. Workspace tree

```
vela/                                  # repo root
├── Cargo.toml                          # [workspace] only — no package
├── Cargo.lock
├── rust-toolchain.toml                 # pinned, reproducible
├── docs/                               # this spec corpus (lives under our_version/ until M0)
├── xtask/                              # architecture + policy linters (see §4)
│   ├── src/
│   │   ├── main.rs                     # dispatch only, < 100 lines
│   │   └── checks/                     # one file per check, each < 300 lines
│   └── Cargo.toml
├── crates/
│   ├── vela-span/
│   ├── vela-syntax/
│   ├── vela-diag/
│   ├── vela-hir/
│   ├── vela-types/
│   ├── vela-mir/
│   ├── vela-bytecode/
│   ├── vela-world/
│   ├── vela-text/
│   ├── vela-audio/
│   ├── vela-compile/
│   ├── vela-vm/
│   ├── vela-replay/
│   ├── vela-ui/
│   ├── vela-render/
│   ├── vela-assets/
│   ├── vela-host/
│   ├── vela-plugin/
│   ├── vela-cli/
│   ├── vela-web/
│   ├── vela-lsp/
│   ├── vela-test/
│   └── vela-migrate/
├── examples/
│   └── hello/                          # the canonical smoke-test project
├── tests/                              # workspace-level integration tests
│   ├── golden/                         # diagnostic + bytecode golden files
│   └── stories/                        # sample stories for vela-test
└── .github/workflows/ci.yml
```

`crates/` is flat and alphabetical. **Rank** membership is declared in each crate's
`Cargo.toml` (`[package.metadata.vela] rank = 0..9`, with `adapter = true` for platform and
GPU adapters) and enforced by `xtask`. See `ARCHITECTURE.md §1` for the rank table and the two
rules.

## 2. Per-crate layout (mandatory convention)

Every crate follows the same interior shape. Consistency here is what lets a new person —
or a new agent session — find anything without asking.

```
crates/vela-syntax/
├── Cargo.toml
├── README.md                 # 5–15 lines: what this crate owns, what it does NOT own
└── src/
    ├── lib.rs                # re-exports ONLY. No logic. < 120 lines.
    ├── error.rs              # error types for this crate (if any)
    ├── types.rs              # shared data types with no behavior
    ├── lex/
    │   ├── mod.rs            # facade, < 80 lines
    │   ├── cursor.rs
    │   ├── token.rs
    │   └── indent.rs         # one concern per file
    ├── parse/
    │   ├── mod.rs
    │   ├── parser.rs         # engine, not rules
    │   ├── stmt.rs           # statement rules
    │   ├── expr.rs           # expression rules
    │   └── recovery.rs       # error recovery
    └── tests/                # unit tests split by concern, not one big file
        ├── mod.rs
        ├── lex_tests.rs
        └── parse_tests.rs
```

**Hard rules for every crate:**

- `lib.rs` contains **re-exports only** — no `impl`, no free functions.
- `mod.rs` is a **facade** — declares submodules and re-exports; target under 80 lines.
- No file mixes two phases. Lexing code does not live in `parse/`.
- Shared *data* goes in `types.rs`; shared *behavior* lives with the phase that owns it.
- Crate `README.md` states the boundary explicitly, including non-responsibilities. This is
  how we prevent the "everything crate" from forming.

## 3. File-size budget

This is the mechanism behind VISION Principle 4. It is enforced mechanically, so it cannot
decay silently.

| Artifact | Warn | **Hard fail** |
| --- | --- | --- |
| `.rs` source file | 400 lines | **500 lines** |
| `lib.rs` / `mod.rs` | 80 lines | 120 lines |
| Single `fn` body | 60 lines | 80 lines |
| Single `impl` block | 200 lines | 300 lines |
| `.md` doc | 500 lines | 700 lines |
| `Cargo.toml` `[dependencies]` | 25 entries | 35 entries |

Lines is the coarse metric; the real rule is the one below it.

> **The one-sitting rule.** If you cannot read a file top to bottom and hold it in your head,
> it is too long — regardless of line count. The numbers above are the enforceable proxy.

### 3.1 How to split a file that exceeds budget

Apply these in order; stop as soon as the file fits.

1. **Split by phase.** lex → parse → lower → check → emit are always separate modules.
2. **Split by variant.** One module per statement/expression family
   (`stmt/say.rs`, `stmt/menu.rs`, …) — not one `stmt.rs`.
3. **Split by responsibility.** Separate the data model (`*_ty.rs`) from the algorithms
   (`*_check.rs`, `*_lower.rs`) that operate on it.
4. **Extract a registry.** A growing `match` over kinds is a registry waiting to be born.
   See §4 of `CONVENTIONS.md`.
5. **Extract a sub-crate** only if the above fail *and* the boundary is layer-clean. This is
   the last resort, not the first: crate proliferation is its own cost.

### 3.2 Comments that are allowed

A comment explaining *why* a file is large is not a solution and is not accepted. An
`xtask`-visible `// vela-exempt: <reason>` line is permitted only for generated files and
machine-written tables, and requires a linked issue in the CI output. Exemptions are
reported on every CI run so they stay visible and get paid down.

## 4. `xtask` architecture linters

`cargo xtask <check>` — these are the enforcement arm of this document. Each check lives in
its own file under `xtask/src/checks/`.

| Command | Enforces | Failure mode |
| --- | --- | --- |
| `check-layers` | `ARCHITECTURE.md §1` rules 1 and 2 | An upward or same-rank edge, or an adapter reachable from a rank ≤ 7 crate |
| `check-file-size` | §3 budget table | File/fn/impl over hard limit; prints the split recipe to try |
| `check-facade` | §2 `lib.rs`/`mod.rs` rules | Logic found in a facade file |
| `check-exemptions` | §3.2 | Exemptions lacking an issue reference; lists the current exemption count |
| `check-diag-codes` | `spec/LANGUAGE.md §8` | Duplicate, unregistered, or gap-invalid diagnostic code |
| `check-registries` | `CONVENTIONS.md §4` | A dispatch `match` exists outside its registry module |
| `check-determinism` | `RUNTIME.md §4` | Banned types/methods found in source (see §4.2) |
| `check-abi` | `TOOLING.md` / `PLUGIN` surface | A plugin ABI change that was not accompanied by a version bump (from M13) |

### 4.1 How rank checking works

Each crate declares its rank in `Cargo.toml`:

```toml
[package.metadata.vela]
rank = 4
```

`xtask` reads each workspace member's `Cargo.toml` directly and inspects its dependency
entries. Only **workspace-internal** edges matter (external crates such as `serde` have no
rank), so there is no need for a resolved dependency graph — which conveniently means the
check needs no `cargo metadata` invocation and runs in milliseconds.

It then asserts two rules from `ARCHITECTURE.md §1`:

1. **Rank:** every internal edge points to a strictly lower rank.
2. **Adapter:** no crate with `rank <= 7` depends on an adapter crate.

A violation fails the build naming the offending crate, the offending dependency, and the
exact `Cargo.toml` line to change.

Adapters are declared with a second key:

```toml
[package.metadata.vela]
rank = 1
adapter = true
```

Rule 2 is the one that keeps the compiler and VM graphics- and platform-free.

### 4.2 How determinism checking works

This is an **`xtask` source scan, not a `clippy.toml` configuration**, and the reason is
concrete: clippy config is workspace-global and cannot express the one legitimate exception
(`vela-host` owns the clock, so `Instant::now` is correct there and banned everywhere else).
A path-aware scan is the only way to encode that.

The scan flags, in every `.rs` file outside the allowlist:

| Pattern | Why banned |
| --- | --- |
| `SystemTime::now`, `Instant::now` | Time must be injected (`RUNTIME.md §4.2`) |
| `thread_rng`, `rand::random`, `RandomState` | RNG lives in `World`; unseeded generators break replay |
| `HashMap<`, `HashSet<` | Unordered iteration; use `IndexMap`/`BTreeMap` |
| `as usize` on a reference/pointer | Address identity must never enter state; use generational ids |

The allowlist (currently `vela-host`) lives in `xtask/determinism-allowlist.toml` alongside a
required justification per entry, so exceptions are explicit and reviewable rather than
implicit.

## 5. Naming

- **Files**: `snake_case.rs`, singular nouns (`token.rs` not `tokens.rs` for a single type).
  Plural is fine for a family (`tokens.rs` if it holds the whole token set).
- **Modules**: named for what they contain, not what calls them. `recovery.rs`, not
  `helpers.rs`. A module called `utils` or `common` is rejected in review.
- **Crates**: `vela-<noun>`. Never `vela-<verb>` — a crate owns a *thing*, not an action.
- **Tests**: unit tests in-crate under `src/tests/`; integration tests in `tests/` at the
  workspace root; golden files under `tests/golden/` with a matching `.expected` sibling.

## 6. CI gates

Every push runs, in order (fastest failure first):

1. `cargo fmt --check`
2. `cargo xtask check-layers check-file-size check-facade check-exemptions`
3. `cargo clippy -- -D warnings` (with the determinism lint set from `CONVENTIONS.md §2`)
4. `cargo xtask check-diag-codes check-registries check-determinism`
5. `cargo test --workspace` (unit + integration + golden)
6. `vela build --verify-reproducible` on the standard example (`BUILD_AND_ASSETS.md §7`)
7. `vela check --format sarif` on a fixture with one deliberate error, validated as JSON
8. `cargo xtask check-abi` (from M13)
9. the wasm engine builds, runs, and is within its size budget (`tools/wasm-smoke.sh`)

No gate is "advisory". A gate that can be ignored by habit is a gate that will be ignored.

**Two jobs run beside the gates**, because they are not policies about the tree:

- `platforms` — a `ubuntu` / `windows` / `macos` matrix that *builds and runs* the standard
  example on each. This is the only honest way to answer "it works on all four targets":
  cross-compiling from Linux proves the code has no platform-specific imports, not that the
  binary links against the platform's libraries and starts. The platform adapters and the
  bundle layout are exactly the parts that are deliberately *not* portable, so they are the
  parts that have to be exercised per platform.
- `budgets` — `cargo run --release -p xtask -- budget`, which refuses under `debug_assertions`.

**Not yet a gate: uploading the SARIF.** `github/codeql-action/upload-sarif` needs code
scanning enabled on the repository, which runs into the same plan limitation as branch
protection (`docs/adr/0001-branching-and-ci.md`). The format is validated here so that
turning the upload on is a repository setting rather than a code change.
