# M12 — Ren'Py migration

**Goal.** A real Ren'Py project can be ported incrementally, with a number that goes to zero.

**Depends on.** M5–M10 (the engine must be real first).

**Crates.** `vela-migrate`.

**Work items.**
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

**Exit criteria.**
- [ ] The sample project transpiles, compiles with zero errors, and runs
- [ ] Report output is deterministic and every entry names a file and line
- [ ] No construct is ever silently mistranslated — verified by a corpus of known-unsupported
      snippets, each producing a report entry
- [ ] **Demo:** `vela migrate tests/fixtures/renpy_sample --report` then `vela run`

**Risks.** Silent mistranslation is the worst possible outcome. Mitigation: the rule "report,
never guess" is a tested invariant, not a guideline.
