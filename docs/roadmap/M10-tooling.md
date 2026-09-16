# M10 — Tooling: LSP, formatter, tests, analysis

**Goal.** The differentiator becomes usable in an editor and in CI.

**Depends on.** M2 (can start early), M5, M7 for goldens.

**Crates.** `vela-lsp`, formatter (landed in `vela-syntax`), `vela-test`.

**Landed early: the formatter.** Item 1's printer and `vela fmt` / `--check` are in, because
nothing else can *establish* the syntax before this milestone: a formatter is what turns "these are
the rules" into text that is checked on every file in the repository. It is in `vela-syntax`
(`format`, `print/`) since it is a function of the tree, and it is pinned by two corpus-wide tests —
`crates/vela-syntax/tests/format_roundtrip.rs` (idempotent, re-parses, keeps every comment) and
`crates/vela-mir/tests/format_equivalence.rs` (lowers to the same program before and after).
`TOOLING.md §3` records the canonical form, including the choices the tree cannot make for itself.
**Item 1 is done except two pieces.** `vela fmt --check` over this repository's own corpus is now a
gate (`check-format`, `REPO_LAYOUT.md §4`): 46 files canonical, and the 15 that do not parse are the
`E0xxx`/`E1xxx` fixtures, which are *meant* to be broken and are counted rather than failed. Running
it by hand the first time found a language bug, not a formatting nit — `not` and `!` had been arriving
in the tree as one variant although they bind at different levels (`not a == b` is not `!a == b`), so
the formatter could not print either faithfully.

**Item 1 is complete.** `--diff` prints the change as a unified diff and writes nothing, and the
pragma is linted (`W4011`), so a region the formatter cannot reach is visible in `vela check` rather
than only in the file. Writing that lint meant *using* a region, which found three bugs in it: the
region's first line was re-indented while its body was not (indentation is structure here, so a nested
block inside a region would have nested differently than its author wrote it), a comment on the line
after a region was printed twice, and a blank line after a `transform` disappeared. The corpus gate
then caught the last of those in the one fixture that had been written around it — which is the
argument for the gate, and the reason the printer's whitespace rules now have tests of their own
instead of being implied by a round trip that was stable and wrong.

**Item 3's foundation is in, and it changed a rank.** The parity criterion — the editor and
`vela check` must publish the same diagnostics — turned out to be *unsatisfiable* as the ranks stood:
what `vela check` publishes includes what the widgets have to say, which is `vela-ui` (rank 8), and
`vela-lsp` was also rank 8, so it could not see `vela-ui` at all. The comment in `vela-cli` that
claimed the language server could check screens was the tell. `vela-lsp` is now rank 9
(`ARCHITECTURE.md §1`), beside `vela-test` — the other crate that assembles what the lower layers say
rather than adding analysis of its own.

With one crate able to see both, the answer lives in one function (`vela_lsp::diagnostics::project`)
that `vela check` and `vela build` both call, and the editor calls per file
(`vela_lsp::diagnostics::file`, which is what an edit wants). The parity test compares the command's
actual JSON output against the same renderer over the server's answer, so a second list growing in
either caller is a failure rather than a drift. Moving the screen checks also fixed a bug they had
carried: they parsed each file with a fixed `FileId`, so every screen warning in a project claimed to
be in the first file and was rendered against whatever line happened to be there.

**Item 2 serves diagnostics, and nothing else yet.** `vela lsp` speaks the protocol on stdin and
stdout: `initialize` (declaring `textDocumentSync: 1` and `positionEncoding: "utf-16"`),
`didOpen`/`didChange`/`didClose`, and `publishDiagnostics` from the same answer `vela check` prints.
The framing is hand-written (`transport.rs`, with a test per way a header can lie), the offsets→
positions conversion is its own module, and the workspace is loaded by *the command line's* loader —
handed to the server as a closure, so the editor and CI cannot disagree about what a project's files
are. `crates/vela-cli/tests/lsp_stdio.rs` spawns the real binary and drives it the way an editor does,
which is what covers the half that only exists in a real run: that the process starts, that the
protocol really goes over the pipes, and that nothing else writes to stdout and corrupts the stream.

**Two of the five features answer, and the rest are still absent on purpose.** Goto-definition and
references read a symbol index (`symbols.rs`) built from what the compiler already knows: `vela-hir`
records every definition with its span and every `jump`/`call` with its own, and resolution goes
through `target_of` — the one function that decides what a reference points at — so the editor cannot
disagree with the checker about where a `jump` lands. Both work across a module boundary, which is the
case the exit criterion names. Hover, completion, and rename stay out of the capabilities until they
answer: hover and completion need the type and the scope at an offset, which is a question
`vela-types` has to answer, and rename needs a name's own span rather than its statement's, which is a
syntax-tree change rather than a language-server one.

Building this found a bug worth recording, because it is the class the parity criterion exists for.
Documents arrive as URIs, and the first version named them relative to the *project* root — so a
session in the editor saw the module `src.main` where `vela check` saw `main`, and every cross-module
reference in the editor would have failed to resolve while the command line resolved them all. The
server now names files relative to the project's `src/`, the same rule `vela check` uses, and the
integration test runs against a real project so that rule is exercised rather than assumed.

**Input from the Ren'Py reference.** Ren'Py's `developer_tools` and `cli` pages are the closest
thing to a spec for this milestone, and four of its tools are worth copying rather than inventing:

- `renpy lint` — a check list to steal wholesale for `vela analyze`: labels that are never reached,
  a say for an undeclared character, images declared and never used, assets referenced and missing,
  and *a `call` with no `from` clause* (which is how Ren'Py's own build finds the saves that are
  about to break; see `RUNTIME.md §5`).
- `renpy translate` — extraction of translatable strings into a catalogue. Two decisions come with
  it: dialogue is translatable *by default* (no marker in the script), and the message id has to be
  edit-stable, which is the same problem as the save anchor.
- `renpy --warp <label>` — start the story at a label and skip the rest. Cheap, and it is what makes
  a long fixture tolerable to work on by hand.
- `testcase` blocks (`run`, `click`, `type`, `assert`) — a shape worth matching in `vela-test` so it
  reads as familiar to anyone who has written one, even though the runner is ours.

**Work items.**
1. Formatter with the rules in `TOOLING.md §3`; `--check` and `--diff`.
2. LSP server over the query database; the capability list in `TOOLING.md §4`.
3. Diagnostic parity test: LSP diagnostics must equal `vela check` output exactly.
4. `vela-test`: scripted input, world assertions, command assertions, story-graph assertions,
   locale sweep, accessibility sweep (`TOOLING.md §5`).
5. Golden frame rendering with tolerance.
6. `vela analyze`: story graph (`dot`/`json`/`html`), dead labels/ends, unused assets, asset
   budget, variable reachability (`TOOLING.md §7`).
7. `vela doc` generating widget/effect/action/diagnostic references from their schemas.

**Exit criteria.**
- [ ] Hover, completion, goto-def, references, rename all work across module boundaries in a
      multi-module fixture
- [ ] `vela fmt --check` on the entire corpus is clean and idempotent
- [ ] `vela test` runs a suite headless in CI and fails a deliberately broken story
- [ ] `vela analyze --format json` output is deterministic across runs (diffed in CI)
- [ ] A newcomer can navigate the fixture using only LSP features (documented walkthrough)
- [ ] **Demo:** the LSP walkthrough in the README, plus `vela test && vela analyze`

**Risks.** LSP correctness depends entirely on the M2 query database being right. Mitigation:
the parity test makes any divergence between editor and CLI a hard failure.
