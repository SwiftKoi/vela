# M1 — Front end: lexer and parser

**Goal.** `.vela` source becomes a CST, with precise diagnostics and error recovery.

**Depends on.** M0.

**Crates.** `vela-syntax` (`lex/`, `parse/`).

## Work items
1. Lexer: UTF-8, comments, string escapes, numbers, indentation → `INDENT`/`DEDENT`
   (`LANGUAGE.md §2`), with `E0xxx` diagnostics.
2. Cursor + token stream with one-token lookahead (needed for the say-statement ambiguity).
3. Parse rules split by family: `stmt/say.rs`, `stmt/menu.rs`, `stmt/control.rs`,
   `stmt/show.rs`, `expr.rs`, `types.rs`, `decl.rs` — **one file per family from the start**.
4. Error recovery: on a syntax error, skip to the next statement boundary and continue, so one
   typo does not produce a cascade.
5. CST types in `types.rs`; `lib.rs` remains re-exports only.
6. Golden corpus `tests/golden/parse/` + `xtask bless`.

## Exit criteria
- [x] `vela check` on `examples/hello` reports zero diagnostics
- [x] Parse golden files exist for every statement family in `LANGUAGE.md §3` (23 inputs)
- [x] Each `E0xxx`/`E1xxx` code has a golden with the exact span underlined (14 inputs, and
      `parse_golden::every_lexical_and_syntactic_code_has_a_golden` enforces the coverage)
- [x] A 1000-line malformed file yields a bounded, non-cascading diagnostic count
- [x] **Demo:** `vela check --format human examples/hello`

## Status

Complete. 94 tests, 7/7 `xtask` checks, clippy clean with `-D warnings`.

## Found during implementation

Writing a parser is the first real test of a grammar, and
five holes surfaced:

- `pause` and `transform` were used by the grammar but missing from the reserved-word list,
  so they lexed as identifiers.
- `with_stmt` was listed among the statement alternatives but never defined.
- `match_expr` was referenced from `primary` and never defined, though §10 defers
  match-as-expression.
- `??` was documented in §5.3 and absent from the expression grammar.
- Colours were written `#rrggbb`, which collides with the comment marker — every colour in
  the specs was unparseable. They are now `0xrrggbb`, with hex literals in the language.

The conditional also became a *suffix* on `expr`, removing an ambiguity where `a if c else b`
could have been read as two adjacent expressions.

Two guards were wrong, and were fixed rather than worked around: `check-file-size` counted
braces inside string literals, and `check-registries` could not tell a lexical table from
domain dispatch (now exempting `vela-syntax`, whose grammar is closed).

## Risks

Indentation handling is the classic source of subtle parse bugs. Mitigation held:
the lexer emits explicit `INDENT`/`DEDENT` tokens and no parser rule inspects whitespace.
