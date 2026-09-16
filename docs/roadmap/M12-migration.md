# M12 — Ren'Py migration

**Goal.** A real Ren'Py project can be ported incrementally, with a number that goes to zero.

**Depends on.** M5–M10 (the engine must be real first).

**Crates.** `vela-migrate`.

## Work items
1. `.rpy` parser sufficient for the supported subset (`TOOLING.md §8`).
2. Transpiler for labels, say, menu, flow, show/hide/scene/with, define/default, character,
   image, basic screens, styles, transitions.
3. Expression translator for the Python subset that maps to Vela expressions.
4. **Report generator**: every unsupported construct as `file:line` + original + reason.
5. `--strict` mode turning any unsupported construct into a non-zero exit.
6. Compat shim for runtime behaviors that cannot be expressed in script (transition timing
   defaults, `renpy.*` calls that map to capabilities).
7. A ported sample project in `tests/stories/migrated/` that runs end-to-end and passes the
   `vela test` suite.

## Exit criteria
- [x] The sample project transpiles, compiles with zero errors, and runs — verified on the real
      thing: `renpy-8.5.3-sdk/the_question` migrates to `examples/the_question_migrated`
      (gitignored; it is generated, and it carries someone else's art), which `vela check`s with
      **no problems** and plays headless through all 91 commands.
      `crates/vela-cli/src/tests/migrate_tests.rs` walks the same journey on a fixture that *is*
      committed, including the check and the run.
- [x] Report output is deterministic and every entry names a file and line —
      `crates/vela-migrate/src/tests/transpile_tests.rs::a_migration_is_deterministic` compares two
      runs byte for byte, and every `Report::Entry` carries `file` and `line`
- [x] No construct is ever silently mistranslated — the corpus in
      `transpile_tests::unsupported_constructs_are_reported_with_their_line`: ten known-unsupported
      snippets, each asserted to produce an entry naming its line
- [x] **Demo:** `vela migrate <project> --report`, then `vela check` and `vela run` on the output
      (`migrate_tests.rs` drives exactly that, because the real sample's assets are not ours to
      commit)

## Status

The **first pass** is in: `vela migrate` reads a project's story files, transpiles the constructs
`TOOLING.md §8` lists, and reports everything else with `file:line`, the original text, and what to do
about it. On the reference project that is seven entries — four whole-file reports for Ren'Py's own
`gui`/`options`/`screens`/`testcases`, two `_()` translation markers, and one label renamed to escape
a namespace collision — and the story itself migrates without a complaint.

Two findings were engine work rather than translation work, and both were worth having on their own:

- **Text tags had to exist.** The sample's ending is `"{b}Good Ending{/b}."`, and `{` was an error.
  Reporting it would have blocked a construct the engine can carry, so tags are now parsed and
  validated (`E0010` for one outside the vocabulary), kept in the string, and read by the presenter
  (`LANGUAGE.md §1`, `BYTECODE.md §3.3`). Bold is drawn by a second pass a hair to the side, because
  the project ships one font face; italic is drawn plain until a shear exists — a renderer gap, named
  in `vela-render/src/text.rs` and asserted in `present.rs` rather than hidden.
- **Ren'Py's namespaces are not Vela's.** `label book` and `default book` are both ordinary in Ren'Py
  and `E2003` in Vela. A colliding label is renamed (`book` → `book_label`), deterministically and
  with a report entry; the alternative was a migrated project that does not compile.

The reader's structure is indentation, and getting that right was most of the debugging: a blank line
after `if h.who:` — Ren'Py's own `screens.rpy` is full of them — ended the block, which sent the rest
of a screen to the enclosing file and turned a 1,500-line file into 596 report entries.

## Still open

- **Work items 2–7.** The expression translator is a whitelist rather than a Python subset; `renpy.*`
  calls have no shim; local labels (`label .quiet_morning`) are reported rather than translated, and
  the note below argues they belong in the language before they belong here.
- **Assets are inventoried, not copied**, and image declarations are not generated. Ren'Py derives
  image names automatically from filenames, so reproducing that is the first piece of asset work —
  and it is blocked upstream for the sample by two importers that do not exist: JPEG (four
  backgrounds) and Opus (one track).
- **The ported sample is not under `tests/`** as work item 7 asks. It cannot be: the sample is
  Ren'Py's own project, with Ren'Py's art. What is committed instead is the migration *test*, and
  `examples/the_question_migrated` is regenerated on demand.
- **Two things decided early, still decided, still not built** — both *additive*, which is why they
  did not have to be settled before the syntax freeze. **Local labels**, `label .quiet_morning:`
  scoped to the enclosing label, which Ren'Py scripts use constantly for a chapter's sibling scenes;
  they are also the natural prefix for the statement identity `RUNTIME.md §5` is missing. And **a
  story-level hook that runs on load**, Ren'Py's `after_load` label: where a *content* fix lives
  ("chapter 4 no longer uses that flag, go to the revised scene") as opposed to the Rust-side schema
  migration `vela-replay` already does. Ren'Py's `block_rollback()` is the detail to copy when it
  lands.

## Risks

Silent mistranslation is the worst possible outcome. Mitigation: the rule "report,
never guess" is a tested invariant, not a guideline.
