# M3 — MIR and optimization passes

**Goal.** Checked programs lower to a typed CFG that the back end can consume.

**Depends on.** M2.

**Crates.** `vela-mir`.

**Work items.**
1. MIR data structures exactly as `BYTECODE.md §2` (`Module`, `Body`, `Block`, `Stmt`,
   `Terminator`, `Value`).
2. Lowering split by construct: `lower/stmt/`, `lower/expr/`, `lower/decl.rs`.
3. Story constructs lower to `JumpLabel`/`CallLabel`/`Dispatch` so `vela-vm` never needs to
   know about labels as a concept.
4. Pass pipeline registry (`CONVENTIONS.md §4.5`) + the five initial passes from
   `BYTECODE.md §2.1`.
5. MIR pretty-printer for debugging and goldens.
6. Differential harness: run the corpus with and without passes, assert identical observable
   behavior (command stream + final `World`). This harness is reused at M5.

**Exit criteria.**
- [x] Every construct in `LANGUAGE.md §3` lowers to MIR with a golden pretty-print — 13
      entries in `tests/golden/mir/`, and `every_mir_form_appears_in_the_corpus` fails if any
      MIR form stops being produced by any of them
- [x] Differential harness green at `-O0` vs `-O2` on the whole corpus — the command stream,
      the final `World`, and the outcome must all match, under two answer scripts
- [x] A new pass can be added by touching only `opt/<name>.rs` + `opt/registry.rs` —
      enforced rather than asserted: three tests check that every registered pass has a file,
      a registry entry, and a golden
- [x] **Demo:** `vela check --emit mir examples/hello`

**Status.** Complete. 204 tests, 7/7 `xtask` checks, clippy clean with `-D warnings`.

**The spec could not express the language.** Building MIR against `BYTECODE.md §2` turned up
eight gaps where the draft had no way to say something `LANGUAGE.md §3` requires. Each was
fixed in the spec and is covered by the corpus:

| Gap | What was missing |
| --- | --- |
| Aggregates | No statement could *build* a list, map, struct, or variant — so a program could not create any of them |
| Optionals | `??` is in the language; `IsNone` and `Unwrap` did not exist |
| Length | `for` had no lowering, because nothing could ask how long an aggregate is |
| Reading a `default` | A default is a *place*, not a value, and there was no operand for it |
| `Module.pool` | `Const(ConstId)` indexed a pool the `Module` did not have |
| Indirect calls | `Call { func: FuncRef }` cannot call a lambda held in a variable, which the grammar allows |
| `Dispatch`'s enum | Rule 6 requires a table to cover "the enum's declared variant range" — unanswerable without knowing which enum |
| `Body.spans` | A table indexed by an id only lowering knows, with no consumer at MIR |

**The syntax tree could not be lowered at all.** `Expr::Int { span }` carried a *position*
and not a value, so `42` was unrecoverable once parsing was done. Literals now carry what
they are; the parser already read the text to validate it.

**`LANGUAGE.md` has two defects of its own**, both found here:

- §4.4's `match` example is unparseable. Patterns are `_`, `Name`, and `Name.Name` — so
  `when 0:` matches nothing, and `when x if x > 5` names a *variant* `x` rather than binding
  a name. The example shows a value match, which the grammar does not have.
- §7.2 and §7.3 give no rule for what a `const`, `default`, or field default may be
  initialised with. A save schema derivable *from the source* (`§7.2`'s stated goal) requires
  a constant, so that is the rule: literals, arithmetic over them, and references to earlier
  constants. Anything else is `E2004`.

**Found by building it.**

- **`check` never saw lowering's diagnostics.** `E2004` was rendered by `--emit mir` and
  *reported nothing* on the ordinary path — the exit code said the project was fine. `check`
  now reports everything the compiler knows about a file, taking the environment and
  lowering's findings from the `mir` query rather than building a second environment that
  could disagree with it.
- **Two ordering bugs in the same shape.** `for_` and `menu` both evaluated a value or
  captured a block *after* opening the loop's blocks — and `open` moves the insertion point,
  so the value was built in the wrong block and the dispatch was sealed in a choice's block
  instead of the menu's. Both lowered to plausible-looking MIR that faulted at run time. The
  `Builder`'s debug assertion caught the second; the differential harness caught the first.
- **A `const` reference lowered to `none`.** `read_name` handled locals and defaults and
  returned `Const::None` for everything else, so `LIMIT + 1` was `none + 1`. Fixed, and the
  lowering order changed with it: declarations are processed before any body, because a use
  of a constant is replaced by its value and a body lowered first has no value to use.

**Risks.** The risk named here was that lowering story constructs into generic control flow
would lose clarity. It did not: `JumpLabel` and `CallLabel` stayed first-class, and the story
graph is recoverable from MIR by inspection — the printer shows it directly. The actual risk
was **block construction ordering**, which is invisible in the printed output and only shows
up when the program runs. That is the argument for the differential harness existing at M3
rather than at M5 where the roadmap put its reuse.
