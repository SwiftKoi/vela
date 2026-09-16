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

**Three of the five features answer.** Goto-definition, references, and rename read a symbol index
(`symbols.rs`) built from what the compiler already knows: `vela-hir` records every definition with its
span and every `jump`/`call` with its target's, and resolution goes through `target_of` — the one
function that decides what a reference points at — so the editor cannot disagree with the checker about
where a `jump` lands. All three work across a module boundary, which is the case the exit criterion
names: renaming a label edits the declaration and every reference through it, in both files.

Two things had to become precise for rename, and both were worth having on their own. `JumpStmt` now
records its target's span — the parser had it and threw it away — so a reference is a *range* a tool
may replace rather than a whole statement it could only jump to; and a reference's span is the last
segment of its path, because `jump main.tally` renames to `jump main.glade`, not to a bare name that
would move the reference to another module. A declaration's own name is still *recovered* from its
first line (the tree records `label start:` as one span), which is text arithmetic and is pinned by a
test over every `DefKind`; recording name spans in the tree is the better fix and the next one.

**Hover answers, from two sources.** The symbol index says what kind of thing a name is and where it is
declared; the checker says what type it has, which is the only way to answer for a local whose type
nobody wrote down (`var total = spend(saved, title)`). That second source is `vela_types::at`, and it is
a *mode of the checker's own walk* rather than a second traversal: the scope rules — a `var` is visible
from where it is written to the end of the body — are exactly what would drift, and a hover that called
a name unknown where the checker accepts it would contradict the diagnostics in the same window. The
innermost expression containing the offset wins, so a caret on `r` in `r.name` says `Route` and a caret
inside `name` says `str`.

**Completion answers from three lists, chosen by position and not by prefix.** At the start of a line
inside a `screen`: the widgets, because that is the only thing that can begin such a line. After `jump`
or `call`: labels — this module's, and the imported ones qualified the way a reference has to be written.
Everywhere else: the names in scope, by the checker's rule, plus the names the module declares.

The middle one is the design decision worth keeping. Offering *every* label in a project would pass a
naive test and be exactly the flat namespace `LANGUAGE.md §6` exists to avoid, so the test asserts that
a label in a module nothing imports is **not** offered. The first one is decided by two questions, both
needed: inside a screen, and at the start of a line — because inside that screen's `if` condition the
answer is an expression rather than a widget.

The scope itself is `vela_types::scope_at`, the same walk as `at` with the other question: it snapshots
the scope before *and* after each statement, because a cursor can be inside a statement (which sees the
scope before it — a `var` is not in scope in its own initialiser) or after it (which sees what the
statement left behind). One snapshot per statement misses every name declared on the line above the
cursor, which is the case that matters most.

Still missing from `TOOLING.md §4`, and each needing the vocabulary of whatever precedes the cursor
rather than the scope: props after a widget name, enum variants after a `.`, and screen actions. A gap
rather than a lie: `completionProvider` declares no trigger characters, because `.` would otherwise
promise a member list this server does not have.

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

**The test surface is in, and the runner is next.** `TOOLING.md §5` puts tests in `.vela` files beside
the story, so `test` items parse, print as canonical, and are typed: an `expect` that is not a `bool` is
`E3007`, a `choose` that is not text is the same, a `run from` resolves like any other reference
(`E5003`, or `E2002` for a module the file did not import), and an unknown directive is one diagnostic
naming the line. `test` is a keyword; `run`, `advance`, `choose`, `expect`, and `cover` are contextual
names, so a story keeps all five as labels, variables, and functions — `LANGUAGE.md §7.6` records the
trade, which `LANGUAGE.md §7.0` has four bug reports about making the other way.

**What the runner needs, read off the machinery that already exists.** Four findings, recorded because
each one changes the shape of the work and none is visible without reading the VM:

1. `vela_vm::driver::run` drives a story *to completion*, which is what a game does and not what a test
   does: `advance 4` means "answer four commands, then stop and assert", and `driver::replay` cannot be
   asked to stop. So `vela-test` owns the step loop — `Vm::run`/`resume` with a pending answer, roughly
   the twenty lines `drive` is built from — and the driver stays what it is good at.
2. `Command::Menu` carries its `choices` with their text, so `choose "Explore"` is a lookup rather than a
   convention, and a text that matches nothing is a failure that can print the options which *were*
   offered. That is the honest version of "the script and the story stopped agreeing", and it is the
   first thing a deliberately broken story can be made to fail on.
3. An `expect` has to be evaluated by **the VM**, not by a second evaluator over the tree. The
   expression is already typed (the checker does that now) and lowering already knows how to build code
   for an expression, so the runner compiles each assertion as a function returning `bool` and calls it.
   Writing a small interpreter in `vela-test` would be a second answer to "what does this expression
   mean", and `visited(forest.river)` needs the world, which only the VM has.
4. `driver::replay` is free determinism for a test: run it, keep the log, replay the log, assert the
   world matches. `RUNTIME.md §4.1` promises exactly that, and a runner that never replays is a runner
   that does not notice when the promise breaks.

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

**The walkthrough is a test, not a document.** All five features answer now, so
`docs/guides/lsp-walkthrough.md` walks a newcomer through them over `examples/standard` — the real
example rather than a fixture — and every step of it is a test in
`crates/vela-cli/tests/lsp_walkthrough.rs`: goto-definition from a `jump` into `chapters/street.vela`,
hover naming the kind and the file, references from a declaration reaching the two qualified callers in
the file that imports it, the completion list, and a rename editing both files. A document is the one
artefact in a repository that cannot fail a test, so the guide can only drift by being edited in the same
commit as the behaviour.

Writing it down found one thing the feature did not do. Typing `jump ` and asking for completion offered
*in-scope names*, because an unfinished transfer does not parse: there is no `Stmt::Jump` in the tree for
the caret to sit in, which is exactly the moment the author is asking what can go there. The context is
now decided from the text as well — the last word before the caret being `jump` or `call` — which is the
one place in completion where the tree is not the best available answer, and the reason is that the file
is half-written by definition.

Probing the server by hand for the guide's numbers is worth doing again for the next feature: the guide's
first draft claimed a list of four labels where the server answers nine, and a rename touching one file
where it touches three. Both were wrong in the direction of *understating* the feature, which is a
failure mode no unit test catches.

Two gaps that the walkthrough records rather than hides. Completion after a `.` (enum variants, struct
fields, module members), widget properties, and screen actions are still missing from `TOOLING.md §4`'s
list; and hovering `directions` in the example says `list<?>` where `list<Direction>` is meant, because a
struct constructor's type is not worked out inside a collection literal — a checker gap, in a feature
that otherwise answers honestly.

**Exit criteria.**
- [x] Hover, completion, goto-def, references, rename all work across module boundaries in a
      multi-module fixture
- [x] `vela fmt --check` on the entire corpus is clean and idempotent (the `check-format` gate over 46
      canonical files; idempotence and re-parsing are pinned by `vela-syntax/tests/format_roundtrip.rs`)
- [ ] `vela test` runs a suite headless in CI and fails a deliberately broken story
- [ ] `vela analyze --format json` output is deterministic across runs (diffed in CI)
- [x] A newcomer can navigate the fixture using only LSP features (documented walkthrough:
      `docs/guides/lsp-walkthrough.md`, every step of it a test)
- [ ] **Demo:** `vela test && vela analyze` (the LSP walkthrough is in the README)

**Risks.** LSP correctness depends entirely on the M2 query database being right. Mitigation:
the parity test makes any divergence between editor and CLI a hard failure.
