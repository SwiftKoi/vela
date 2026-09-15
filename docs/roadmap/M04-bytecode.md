# M4 — Bytecode, verifier, and codec

**Goal.** Verified bytecode modules in a versioned container.

**Depends on.** M3.

**Crates.** `vela-bytecode`.

**Work items.**
1. `OpSpec` table (`op.rs`) as data; assembler and disassembler both read it.
2. MIR → bytecode codegen, including jump-table construction for `Dispatch`.
3. Verifier implementing all eight rules in `BYTECODE.md §4`.
4. `.velac` container reader/writer with the header from `BYTECODE.md §3.1`.
5. Command and effect **schema** tables (variants + field schemas), plus `E7102` handling.
6. Debug info: span table, slot names, line table (`BYTECODE.md §5`).
7. Golden disassembly corpus + a verifier-rejection corpus.

**Exit criteria.**
- [x] Verifier rejects, with `E6xxx`, a hand-crafted module for each of its eight rules —
      `tests/rejection.rs`: ten modules, one per rule, plus a well-formed one so the tests
      are rejecting something rather than everything
- [x] Round-trip test: `decode(encode(m)) == m` for the whole corpus — all 13 MIR entries,
      and `encode(m) == encode(m)` as well, because a round trip through a nondeterministic
      encoder would still pass
- [x] Backward-compat test: `tests/golden/velac/hello-v1.velac` loads and verifies
- [x] Adding a command variant requires no change to `vela-vm` — `known_commands` reads
      `CommandKind`'s table, and a test-only variant is reported by `E7102` without anything
      else being told about it
- [x] **Demo:** `vela check --emit disasm examples/hello`

**Status.** Complete. 230 tests, 7/7 `xtask` checks, clippy clean with `-D warnings`.

**The instruction table was missing eleven ops.** Each was found by asking what
`LANGUAGE.md §3` needs, and the cluster says something about how §3.2 was written: it reads
like a list of what an imaginary program uses rather than a derivation from the language.

| Added | Why |
| --- | --- |
| `Unwrap` | `UnwrapOr` existed and nothing *asserted*; `??` branches on `IsNone` and needs the payload with no fallback |
| `ToBool` | The checker accepts `bool(x)`; the table had `ToStr`, `ToInt`, `ToFloat` |
| `CallValue`, `ConstFn` | The grammar has lambda literals; the table could compile one and never call it |
| `LtS`, `LeS`, `GtS`, `GeS` | `EqS` existed and no ordering for strings, which the checker returns `bool` for |

**Two encoding decisions the spec left open.** `OperandKind` is the "operand kind" §3.1
fixes as a byte and never enumerates. And a dispatch table travels *with* its instruction
rather than trailing it as loose bytes — otherwise finding a table means knowing how an
instruction is laid out, which is the coupling the kind byte exists to prevent.

**`Effect::Update`,** because aggregates are values and not references: `xs[0] = v` has to
*produce* a list for the compiler to store back. An in-place update would need the runtime
to know where the aggregate came from, which is exactly the aliasing a story engine should
not have.

**Found by running it.**

- **The verifier did not terminate**, and the fix for it silently failed to land once: I
  applied it in a command whose output the test timeout discarded, read "ok" from a run that
  had printed nothing, and then spent a budget instrumenting the wrong hypothesis. The bound
  added *specifically to make a compiler bug surface as a rejection* was never in the file.
  The isolating experiment — compile one file, do not verify — would have named the
  verifier in seconds.
- **`Operand::u32()` returns a `Pair`'s first element**, but for `Cmd` the second is the
  arity, so a five-argument `say` was verified as taking its variant id's worth of operands.
- **A `Place::Index` store picked its instruction from the subscript's type** rather than
  the aggregate's.
- **A verifier that stops the walk at a jump *and* follows it** leaves the jump's target
  unvisited, so everything it reached looked like orphaned code.

**Process.** `ROADMAP.md` says the rejection corpus is written **first**, against a failing
test. I wrote the verifier first and the corpus nearly last — the exact inversion the plan
warns against, and two of the ten corpus tests failed on first run. The plan was right.

**Found later, by the first example that used them (M9).** Three faults on paths the corpus never
took, each of them a *representation* disagreement rather than a missing feature — and the corpus
could not have caught them as written, because it is MIR-shaped and these are all about which
instruction gets emitted:

- **`concat` had three answers to one question.** The instruction table declared it
  count-carrying, the emitter wrote no count, and the machine assumed two. The verifier believed
  the table, counted a value left on the stack, and reported `E6001` for **any loop whose body
  presented a command** — a menu, a `say`, a `pause`. It is a two-string instruction, and now says
  so in the table, the emitter, and the machine.
- **`ty_of` did not read the constant pool**, so every literal was `Ty::Unknown` — and the operator
  emitter *picks its instruction* from that type. `"i is " + text` compiled to `add.i`; `1.5 + x`
  compiled to integer addition. It reads the pool now, which is also why the golden container moved
  (below).
- **A constant variant emitted its operands swapped.** `Ending.cold` used as a *value* wrote
  `(variant, 0)` where `EnumNew` reads `(enum, variant)` everywhere else, dropping the enum: it
  built whichever enum sat at index `cold`'s position. A payload-carrying variant takes the other
  path and was always right, which is why `matching.vela` never saw it.

`tests/golden/velac/hello-v1.velac` moved by one byte for the second of those, and the diff is
worth stating: `say`'s command schema now records its real argument types (`Str`, `None`) where it
recorded `Unknown`. That is the fix showing up in the golden, not format drift — blessed in the
same commit, and the only golden that changed.
