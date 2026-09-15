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
