# Tooling Specification

Status: **draft, normative for M10–M12.**

Tooling is not a nice-to-have here — it is the product (`VISION.md §3.1`). The engine exists
so that a story can be checked, tested, refactored, and debugged like software. This document
specifies the surface a user actually touches.

## 1. CLI

Binary: `vela`. Subcommands are registry entries (`CONVENTIONS.md §4.7`).

| Command | Purpose | Notable flags |
| --- | --- | --- |
| `vela new <name>` | Scaffold a project | `--template minimal\|standard\|rich` |
| `vela run` | Run with hot reload | `--headless`, `--start <label>`, `--seed <n>` |
| `vela check` | Compile + lint, no run | `--deny-warnings`, `--format human\|json\|sarif`, `--watch` |
| `vela build` | Package for targets | `--target web,win,mac,linux,android`, `--release`, `--patch-from <prev>`, `--patch-out <dir>`, `--verify-reproducible` |
| `vela patch apply <patch> <bundle>` | Apply a delta patch to an existing build | — |
| `vela test` | Headless story tests | `--update`, `--seed`, `--filter`, `--a11y` |
| `vela analyze` | Story graph + asset reports | `--format dot\|json\|html`, `--dead-labels`, `--assets` |
| `vela fmt` | Canonical formatter | `--check`, `--diff` |
| `vela doc` | Generate docs from schemas | `--out docs/`, `--open` |
| `vela lsp` | Language server on stdio | `--stdio`, `--log <file>` |
| `vela debug` | DAP server | `--port`, `--stdio`, `--start <label>` |
| `vela migrate <path>` | Ren'Py → Vela transpiler | `--report`, `--in-place`, `--strict` |
| `vela doctor` | Environment diagnosis | `--verbose` |

Design rules:
- Every command supports `--format json` where output is machine-consumed.
- Exit codes are stable and documented: `0` success, `1` diagnostics present, `2` usage error,
  `3` internal error. CI depends on this and it never changes.
- `vela check` is the CI entry point; `vela run` is the developer entry point. They share the
  compiler and must never disagree.

## 2. Diagnostics contract

One model, three renderings (`ARCHITECTURE.md §6.1`). Codes are defined in
`LANGUAGE.md §8` and registered centrally; `check-diag-codes` enforces uniqueness, the range
scheme, and that every code has a test.

### 2.1 Human

```
error[E5003]: undefined label `forest.clearring`
  --> chapters/start.vela:42:10
   |
42 |     jump forest.clearring
   |          ^^^^^^^^^^^^^^^ no label `clearring` in module `forest`
   |
   = help: did you mean `forest.clearing`?
   = note: labels in `forest`: clearing, river, camp
```

Rules: always print the code, the primary span, and — where one exists — a machine-applicable
suggestion. "Did you mean" suggestions are edit-distance based over the *actually in-scope*
namespace, not a global list.

### 2.2 JSON
A stable, versioned schema: `{ code, severity, message, spans[], notes[], suggestions[] }`.
Stability is a compatibility promise; a consumer parsing this must not break on upgrades.

### 2.3 SARIF
`vela check --format sarif` uploads directly to GitHub code scanning, so every diagnostic
appears inline in a PR. This is the mechanism that makes "check your story like code" real
rather than aspirational.

### 2.4 Severity and promotion
Every diagnostic has a default severity. A project may promote or demote any code in
`vela.toml`:

```toml
[lints]
W4002 = "error"      # unreachable label — we want this to block merges
W7001 = "allow"      # unused asset — noisy during art production
```

`--deny-warnings` in CI treats all warnings as errors. Demoting an `E` to a warning is
rejected (`E7301`) — errors are errors.

## 3. Formatter

`vela fmt` produces one canonical form of every file. This is not cosmetic: it is what makes
diffs readable and merge conflicts rare in a project with many writers.

The formatter is a **function of the syntax tree** — it reads the tree and nothing else — which is
why it lives in `vela-syntax` (`format`, `print/`) and why two files with the same tree format to
the same text. Both properties are checked rather than intended:
`crates/vela-syntax/tests/format_roundtrip.rs` formats every `.vela` file in the repository twice,
re-parses in between, and requires every comment to survive; `crates/vela-mir/tests/format_equivalence.rs`
lowers each file before and after and requires the same printed program. Stability alone would not
be enough: a printer that dropped a clause is perfectly idempotent.

Rules:
- One statement per line, always (this mirrors `LANGUAGE.md §1.6`).
- 4-space indent per level. Indentation *width* is otherwise free (`E0004` asks only for
  consistency), so the formatter is where 4 becomes the answer.
- Blank lines are the author's. Each one is kept, a run collapses to one, and the formatter adds
  none of its own: a formatter that closed up the paragraphs of a long story file would have
  destroyed something a reader uses.
- Comments are kept, in place (`LANGUAGE.md §1`): one written after code stays on that line, one on
  a line of its own stays on its own line. This is the rule a formatter is most often caught
  breaking, and the reason the lexer records comments at all.
- The right-hand side of an `=` is wrapped at 88 columns by splitting at the *outermost* binary
  operator, never mid-token. The continuation is a trailing `\` (`LANGUAGE.md §1`), and each
  operator leads the line it continues:

  ```vela
  var allowed = has_key and \
      not locked and \
      tries < 3
  ```

  Only a binary chain is wrapped, only at its outermost operators, and an operand that is still too
  long stays too long. That is the whole rule: a wrap has to re-parse to the same expression, and a
  split at a binary operator with a `\` is the only one the language can express.
- Strings are *decoded* by the parser, so the printer must re-escape: `\\`, `\"`, `\n`, `\t`, `\r`,
  and both sigils doubled — `[[` and `{{` (`LANGUAGE.md §1`). Without the doubling a literal `[`
  would return as an interpolation and a literal `{` as a text tag. An escape that did nothing is
  dropped (`\q` is `q`, `\}`, `\[`, `\{`), which is what "normalize redundant escapes" means in
  practice. Text is never reflowed.
- Hex integer literals keep their radix, because colours are written that way (`LANGUAGE.md §1`);
  `_` separators are dropped as decoration.
- A `# fmt: off` region is reproduced verbatim, and an unterminated `off` runs to the end of the
  file. The pragma is **linted** (`W4011`, reported by `vela check`) so that its use is visible in
  review rather than discovered later; both `off` and `on` are reported, because an `on` with no
  `off` above it does nothing at all and a warning is the cheapest way to find that out. The
  comment-to-pragma rule has one definition, on `Comment::pragma` in `vela-syntax` — the formatter
  and the lint read the same function, so they cannot disagree about whether a region was asked for.
- Trailing commas in multi-line lists/maps/params: **not reachable yet**. Nothing the formatter
  emits is multi-line except a wrapped right-hand side, and a wrap never lands inside a list. The
  rule stays here for whatever first produces one.

**Where the tree cannot choose, the formatter decides.** Both spellings parse to the same node, so
each of these costs a diff and is what makes two authors' files converge:

| Written | Canonical | Why |
| --- | --- | --- |
| `pause 1.5` or `wait 1.5` | `pause 1.5` | one keyword per event, and a duration is the common case |
| bare `pause` or `wait click` | `wait click` | a bare `pause` does not say what it is waiting for |
| `column gap 8`, `box at bottom` | `column gap = 8`, `box at = bottom` | a screen arg's value is explicit, so a prop and a flag are told apart |
| `text name style = aside` | `text name, style = aside` | commas separate args, or a bare flag swallows the arg after it |
| `screen pause():` | `screen pause:` | no empty parenthesis pair |

**A file that does not parse is refused, not rewritten.** The tree has gaps where the syntax error
was, so printing it would replace the unparsable part with nothing — the one failure that looks like
success. `vela fmt` reports the diagnostic and leaves the file alone.

`vela fmt` writes the files it changes. `--check` writes nothing and exits non-zero when any file
would change — naming them, which is the form CI runs — and `--diff` writes nothing and prints the
change as a unified diff, for a reader who wants to see it before agreeing to it. Both exit non-zero
when there is anything to do; asking for both at once is a usage error, because they answer
different questions.

The formatter is deterministic and the compiler never depends on formatting — so a file that
fails `--check` is still compilable, and CI's `fmt --check` is a style gate, not a
correctness gate. `vela fmt --check` over *this repository's* fixtures is M10's work: the parse
goldens pin spans, so reformatting a fixture re-blesses the golden that describes it.

## 4. Language Server (`vela-lsp`)

A thin adapter over the incremental query database in `vela-compile`
(`ARCHITECTURE.md §6.2`). The server holds no analysis state of its own.

| Capability | Notes |
| --- | --- |
| Diagnostics (push) | Same codes as `vela check`; must be identical, verified by a parity test |
| Hover | Types of expressions, docs for labels/screens/widgets/effects — the comment block above a declaration (`LANGUAGE.md §1`) |
| Completion | Labels (scoped, not global), screens + their params, widget names, props, enum variants, actions |
| Goto definition | Labels, defaults, structs/enums, characters, images, assets (`@path`) |
| Find references | Across modules — the reason namespacing matters |
| Rename | Safe cross-module rename of labels, defaults, types |
| Signature help | Labels and screens |
| Semantic tokens | Distinguishes labels, characters, defaults, types, assets |
| Inlay hints | Inferred types on `var`; assets resolved on `@path` |
| Code actions | Add missing `match` arms; add missing `use`; extract label; convert `absolute` layout to a container |
| Document symbols / workspace symbols | Labels, screens, types |

Two capabilities are worth calling out because nothing in the VN space has them:

- **Inlay hints for assets**: `@"assets/forest.png"` resolves and hints the imported size and
  format, catching a 12 MB PNG before it ships.
- **Story-aware code actions**: "this label is unreachable" offers to delete it; "this menu
  covers 2 of 4 branches" offers to scaffold the missing ones.

## 5. Test runner (`vela-test`)

Drives the real VM headless (§9 of `RUNTIME.md`). Tests live beside stories.

```vela
# tests/stories/forest.test.vela
test "picking the forest sets trust":
    run from forest.clearing
    choose "Explore"          # selects a menu option by text
    advance 4                 # consume 4 dialogue lines
    expect trust == 1
    expect visited(forest.river)

test "every route reaches an ending":
    cover labels             # asserts every label is executed
    cover variants           # asserts every enum variant is matched
```

Capabilities:

| Feature | Purpose |
| --- | --- |
| Scripted input | Deterministic choice/advance sequences (`RUNTIME.md §8`) |
| Seeded RNG | Reproducible randomness |
| Story-graph assertions | `cover labels`, `cover variants`, `no_dead_ends` |
| World assertions | Typed expressions evaluated against `World` |
| Command assertions | Assert the *exact* command stream (golden) or a shape (matcher) |
| Golden frames | Render at a scripted point, compare to a stored PNG (tolerance-based) |
| Locale sweep | Re-run the suite under every locale, failing on overflow/missing strings |
| Accessibility sweep | `--a11y` walks every screen's focus order |

`vela test --update` re-blesses goldens; CI fails on any unblessed change, so a golden diff
is always reviewed.

> **Implemented (M10).** `vela test` runs a project's `test` items headless, and `--a11y`
> adds the M7 sweep beside them (*"every screen passes the `--a11y` focus-order sweep"*) — the two are
> different subjects with different verdicts, which is why the sweep stays a flag.
>
> Running a suite is `vela-test`, and its shape is worth knowing before reading it. A **step loop**
> rather than `driver::run`: a script answers a bounded number of commands and then asserts, where a
> game plays to the end. **Assertions are compiled**: each `expect` and `choose` becomes a nullary
> function appended to the file it was written in, and the runner reads its value out of the *live*
> world with `Vm::call` — so `visited(forest.river)` needs no second evaluator, and the checker types
> the assertion before it can fail. **A `choose` means the next choice**, not the next command: a
> player hears the dialogue on the way to it, and a text that matches no option fails with the options
> that *were* offered. **`cover labels` drives the run to its end**, because "every label" is a claim
> about a whole playthrough; the labels are recorded by the machine (`Vm::entered_labels`), which is
> observational state and deliberately not part of a save.
>
> **Not yet.** Golden frames (`--update`, `--seed`) and the locale sweep are named and unimplemented:
> rendering at a scripted point and comparing to a stored image is `vela-render`'s to produce, and a
> locale sweep needs the text pipeline rather than the story runner. `cover variants` needs the machine
> to record which enum variants a `match` chose, which nothing does — so a directive this version cannot
> honour is reported as a *note* on the test rather than skipped, because a test that checks less than it
> says is worse than one that refuses to run.

## 6. Debugger (DAP)

A Debug Adapter Protocol server, so debugging works in VS Code and any DAP client — Ren'Py has
no equivalent.

- Breakpoints on labels and on source lines.
- Step over / into / out across the frame stack.
- Inspect `World.defaults`, locals (named via `BYTECODE.md §5` debug slot names), call stack.
- Evaluate expressions in the paused frame using the real type checker — an invalid expression
  yields a normal diagnostic, never an evaluator crash.
- Time-travel: because rollback is snapshot-and-replay, the debugger can step *backwards*
  across commands. This is free from the runtime's design, not extra machinery.

> **Implemented (M11).** `vela debug` serves DAP over stdio (`--port` listens on
> `127.0.0.1` instead), and `vela-debug` implements the list above: label breakpoints
> (`setFunctionBreakpoints`) and line breakpoints (`setBreakpoints`), `next`/`stepIn`/`stepOut`/
> `stepBack`, `stackTrace`/`scopes`/`variables` over the frame's slots and `World`, and `evaluate`.
> The stop policy — which site a breakpoint matches, what a step means across a frame — lives in
> the debugger; the machine stays a pull interface (`RUNTIME.md §9`).
>
> **Not yet.** `reverseContinue` (the ring goes back
> a command at a time but does not record where breakpoints were), conditional and hit-count
> breakpoints, log points, `setVariable`, and restart. A client is not told about them, so it does
> not offer them, and asking anyway gets a failure with a message.
>
> **Note.** `evaluate` answers names with values. Any other expression is *checked* with the real type
> checker and answered with its type (`int (checked; only names are evaluated)`), or with the
> checker's own `Exxx` if it does not check. Compiling a non-name expression into the running story
> would need the machine to call a function with arguments, which it cannot do yet; a half
> evaluator that sometimes invents a value is worse than one that says what it knows.
>
> **Not yet.** `Header::FLAG_DEBUG` is clear
> (`BYTECODE.md §5`), so the machine reports no span or slot name and `setBreakpoints` answers
> `verified: false`. Label breakpoints and instruction-level stepping still work.

## 7. Analysis (`vela analyze`)

Turns the compile-time story graph into human decisions.

| Report | Answers |
| --- | --- |
| Story graph | `--format dot\|json\|html` — the full branch structure as a navigable diagram |
| Dead labels | Labels no path can reach (`W4002`) |
| Dead ends | Paths that terminate without an ending |
| Branch coverage | Read the graph, not the docs: are all routes reachable? |
| Unused assets | Assets never referenced (`W7001`), with total size wasted |
| Asset budget | Per-scene load sizes — catches a 300 MB chapter before release |
| Localization coverage | Untranslated strings per locale, and UI overflow risk per locale |
| Variable reachability | `default`s never read, or read before ever written |

Output is deterministic and diffable, so `vela analyze --format json` can be tracked as a
metric over time in CI — "our dead-end count went up by 3 this week."

> **Implemented (M10).** `vela analyze [path] [--format text|json|dot]` reports the story graph:
> every label in every module as one graph, with the edges resolved by the compiler's own resolver, and
> whether a run from the manifest's entry can reach each one. The JSON is the documented surface
> (`tests/golden/analyze/` is its golden), `dot` draws it for `dot -Tsvg`, and `text` is one line per
> label — which is the form a diff reads best.
>
> Two of the reports above are already the checker's: an unreachable label is `W4002` and a label that can
> end without transferring control is `W4003`. Re-deriving them here would give the project two answers to
> one question, so `reached` is a *graph* fact computed from the same resolver, and the diagnostics stay
> `vela check`'s to report with spans and advice.
>
> **Not yet.** unused assets (`W7001`, which does not exist), per-scene load sizes, localization coverage,
> and variable reachability. Each needs something the project does not have yet — an asset manifest that
> survives a check, a text pipeline, a liveness pass over `default`s — and the omission is named in the
> source rather than left as a section that silently never appears.

## 8. Migration (`vela-migrate`)

`vela migrate path/to/game` transpiles `.rpy` → `.vela` for the common case and reports the
rest precisely.

**Supported in the first pass (M12):** `label`, `jump`, `call`, `return`, say statements,
`menu`, `if/elif/else`, `define`/`default`, `character`, `image`, `show`/`hide`/`scene`/`with`,
transitions, and simple Python expressions that map to Vela expressions (`$` statement blocks →
best-effort function extraction, flagged). **M12.1 added the other two halves of a project**: `gui.rpy`
becomes a theme, and `screens.rpy` becomes the screens and styles that draw with it.

**Reported, not guessed:** anything outside that set produces a report entry with
`file:line`, the original text, and the reason. The rule is simple:

> **Note.** Never silently mistranslate. A wrong automatic translation is worse than an explicit
> "port this by hand", because it fails later and in a save file.

The report is itself a work item list, and `--strict` turns any unsupported construct into a
non-zero exit, so a team can track migration progress as a number that goes to zero.

> **Implemented (M12).** `vela migrate <path> [--out <dir>] [--report] [--strict]` transpiles a
> project's story files and writes `MIGRATION.md` beside them — the report *is* the rest of the
> work, and a work item list that scrolls off a terminal is one nobody keeps. `--strict` is the
> number a team can gate on. `crates/vela-migrate/src/tests/transpile_tests.rs` holds the corpus
> of known-unsupported snippets the third exit criterion asks for, and
> `crates/vela-cli/src/tests/migrate_tests.rs` migrates a project and then **checks** it, because
> "the output compiles" is not a claim a migrator can make about itself.
>
> **A story file is one that declares a label.** That is the whole test, and it is the right one:
> Ren'Py's story files are exactly the files with labels in them, and every other `.rpy` —
> `options`, `screens`, `testcases` — is engine configuration or the screen language, which Vela
> expresses differently. Those are reported once, by file, rather than per line: a 1,500-line
> `screens.rpy` reported as 596 entries is a report nobody reads.
>
> **`gui.rpy` is translated, since M12.1** (`crates/vela-migrate/src/gui/`), and it is the one place
> where a file's meaning depends on another file's text: `properties gui.text_properties("name")` in
> `screens.rpy` is what says `gui.name_xpos` belongs to the `name` style. So a colour becomes a theme
> token, a font a font token, and a live `<group>_<state>_<prop>` a `style` line — but only when
> `<prop>` is one Vela **paints** (`color`, `background`, `size`, `font`). The GUI's *placement*
> variables (`xpos`, `ypos`, `xalign`, `spacing`, `borders`) are accepted and ignored by a Vela
> style, so they
> are reported, grouped by style, rather than written into a file that would look migrated and draw in
> the wrong place. `gui.init(width, height)` becomes `vela.toml`'s `[project] size`, which is what
> `variant("small")` measures against (`SCREENS.md §2.6`).
>
> **`screens.rpy` is translated too, since M12.1's item 17** (`crates/vela-migrate/src/screens/`).
> Which `.rpy` file becomes what is now a table rather than a rule of thumb:
>
> | File | Disposition |
> | --- | --- |
> | a file that declares a `label` | **Translated** as a story module (`src/<name>.vela`). |
> | `gui.rpy` | **Translated** into the theme and its styles, one module with the screens (see above). |
> | `screens.rpy` | **Translated** into `screen` and `style` declarations, in the same module as the theme. |
> | everything else | **Reported**, once by file: engine configuration, `testcases.rpy` (a `test` decl lands with M12.1's item 18), `tl/**`. |
>
> The widget tree comes across — `hbox`/`vbox`/`add`/`null`/`fixed`/`frame`/`window`/`label`/
> `textbutton`/`vpgrid` all lower mechanically — as do the conditions, the loops, the `use`
> composition, the `default` variables and the actions Vela has a name for (17 of the sample's 20;
> `ShowMenu`/`Return` are `open_screen`/`close_screen`, and `Start`, `MainMenu` and `InvertedSelected`
> are reported). A style's *paint* is carried and its *placement* is not: `xpos 240` is a skin's
> business (`SCREENS.md §4.2`), while a matched `xalign`/`yalign` pair — written on a widget's line or
> as two lines under it — is one of Vela's nine anchors.
>
> Three things are worth knowing before reading a migrated screen:
>
> - **A style Vela cannot express is not declared, and a widget that named it is reported.** Ren'Py's
>   styles are mostly a `properties gui.button_properties("button")` splat and placement, neither of
>   which a Vela style carries, so they arrive empty and are dropped — which is why a `style "namebox"`
>   prop is reported rather than becoming a reference to a style nothing declares (`E5007`).
> - **A style and a screen cannot share a name** (`E2003`): the screen keeps it and the style is
>   renamed `…_style`, with an entry, because a `use` and an `open_screen` are what refer to a screen.
> - **A named action argument is dropped with an entry.** Vela's actions take positional arguments
>   only (`E5013` counts them), so `Quit(confirm=False)` becomes `quit()` and the word is reported; an
>   argument that reads `config.`/`renpy.`/`persistent.` takes the whole action with it, because an
>   action that plays an asset nobody named is not an action.
>
> What a migrated screen still cannot do is a *list*: `for i in range(6)` is written and walks
> nothing, because nothing in a Vela screen produces a sequence yet (`SCREENS.md §2.4`'s **Not yet.**),
> and the report says so. A data-driven caption (`text i.caption`) is drawn but carries `W4010` — the
> a11y tree is built from the screen's source, which is `SCREENS.md §10`'s own limitation and not a
> migration defect.
>
> **Assets are inventoried, not copied.** An asset in Vela is only meaningful once something
> declares it (`image bg.room = @"art/room.png"`), and Ren'Py declares its images *automatically*
> from filenames — reproducing that is its own piece of work, and the first item on the report.
> There is also a hard reason not to copy blindly: `vela check` **imports** everything under
> `assets/`, so a file no importer claims (a JPEG background, an Opus track) is an error rather
> than a warning.
>
> **Two things the language had to grow, rather than report.** Text tags, because the sample's
> ending is `"{b}Good Ending{/b}."` and reporting it would have been a blocker for a construct the
> engine can carry (`LANGUAGE.md §1`). And a report entry that is *not* a blocker: Ren'Py keeps a
> label and a `default` in separate namespaces and Vela does not, so a colliding label is renamed
> deterministically, with the entry saying so and every `jump`/`call` following it.
>
> **Not yet:** work items 2–7 — the expression translator beyond the whitelist, the compat shim for
> `renpy.*` calls, local labels, the ported sample under `tests/`, and translation extraction.

## 9. Docs (`vela doc`)

Generates reference documentation from the single sources of truth already in the codebase:
widget prop schemas, effect signatures, diagnostic registry, action registry, and public
script APIs. Because it reads the same schemas the compiler uses, generated docs cannot drift
from behavior. This is also what keeps the LSP completion list and the docs identical.

> **Implemented (M10).** `vela doc [widgets|actions|diagnostics]` prints the reference on standard
> output, and `vela doc --out docs/reference` writes it — one file per page, which is the only place in
> this tool that writes anything, because a reference page is an artifact whose whole point is landing
> where a reader finds it. The committed pages are under `docs/reference/`.
>
> What makes the "cannot drift" claim real is a test rather than a promise:
> `crates/vela-cli/tests/doc_golden.rs` regenerates every page and compares it with what is committed, so
> adding a prop, an action, or a diagnostic code and forgetting the reference fails the suite. It also
> asserts that printing a page and writing it agree, since a reader piping the command into a file should
> get the same page a build script does.
>
> **Not yet.** effect signatures, because an `effect` is declared by the *project* — reference for
> one project's effects is that project's code, not engine documentation — and public script APIs, because
> the engine's callable surface is the builtins and those are not a schema.
>
> **Implemented (M11).** A widget or an action is not declared anywhere a name is, so
> neither the symbol index nor the checker knows it; hover answers it from `vela-ui`'s registries, with
> the sentence `vela doc` prints. `Widget::summary` and `ActionDecl::signature` exist so that sentence has
> one spelling rather than two, and the heading on the page is built from the same signature the hover
> quotes. The context is the same rule completion uses — a widget only at the start of a screen's line, an
> action only after the keyword — so a hover cannot call a prop's value a widget.
>
> When the workspace has generated the pages (`vela doc --out docs/reference`), the hover also links to
> the section, with the anchor a markdown renderer would compute from the heading. When it has not, the
> sentence stands alone: a link to a file that is not there is worse than no link, because an editor
> offers to open it and then fails.

## 10. Open questions

1. **Editor beyond LSP** (post-1.0): the GUI editor (`VISION.md §3.6`) is deliberately not in
   the 1.0 scope. The LSP plus the formatter is the 1.0 contract; the editor is a separate
   product built on it.
2. **Snapshot testing of text** (M10): golden frames cover rendering; deciding whether
   per-line text layout goldens are worth their maintenance cost.
3. **Distributed/shared `vela check` cache** (post-1.0): the query database is local; a shared
   CI cache is a possible speedup, not a design dependency.
