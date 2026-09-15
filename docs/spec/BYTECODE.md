# Bytecode Specification

Status: **draft, normative for M3–M4.**

This document defines the compilation contract between the front end (`vela-hir`,
`vela-types`) and the runtime (`vela-vm`). It has two layers: **MIR**, a typed mid-level IR
that is the *stable* contract, and **bytecode**, a concrete stack encoding optimized for a
simple, verifiable interpreter.

The split exists so we can add a register-based backend later without touching the front
end. Everything above MIR is frozen once M3 lands; MIR→bytecode is a replaceable detail.

## 1. Compilation stages

```
CST → HIR → typed HIR → MIR → optimized MIR → bytecode module → verified .velac
        │         │         │           │
        │         │         │           └─ vela-mir::opt
        │         │         └─ vela-mir::lower
        │         └─ vela-types::check
        └─ vela-hir::lower + resolve
```

## 2. MIR

MIR is a **control-flow graph of basic blocks** over **typed slots**. It is not SSA — slots
are mutable and named `_0.._n` — because slot-based IR produces smaller bytecode and far
simpler verifier rules, and we do not need the optimization power of SSA for a story engine.

> **Revised during M3.**  The structures below are what the language actually needed to be
> lowered into. The first draft could not express several constructs in `LANGUAGE.md §3`;
> each addition is marked *added in M3* and is covered by the golden corpus
> (`tests/golden/mir/`) and by `every_mir_form_appears_in_the_corpus`.

```rust
pub struct Module {
    pub name: ModuleName,
    pub structs: Vec<StructDef>,
    pub enums: Vec<EnumDef>,
    pub consts: Vec<ConstDef>,
    pub defaults: Vec<DefaultDef>,   // World state
    pub fns: Vec<Body>,              // includes lifted lambdas
    pub labels: Vec<Body>,
    pub pool: ConstPool,             // added in M3: `Const(ConstId)` had no pool to index
    pub assets: AssetRefs,
}

pub struct Body {
    pub name: Symbol,
    pub params: Vec<Slot>,
    pub ret: Ty,
    pub locals: Vec<LocalDecl>,      // slot id -> type, and the slot's source name
    pub blocks: Vec<Block>,
    pub entry: BlockId,
}

pub struct Block {
    pub id: BlockId,
    pub stmts: Vec<Stmt>,
    pub term: Terminator,
}

// Added in M3: statements carry spans. The spec's `Body.spans: SpanTable` was an id that
// only lowering knew, with no consumer at MIR. A table is genuinely needed at the *bytecode*
// layer, where it is indexed by instruction offset and read by a debugger — and that is
// where it is built.
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

pub enum Terminator {
    Goto(BlockId),
    Branch { cond: Value, then_: BlockId, else_: BlockId },
    Return(Option<Value>),
    JumpLabel(LabelRef),          // story-graph edge
    CallLabel { target: LabelRef, ret: BlockId },
    Dispatch {                    // exhaustive match
        enum_name: String,        // added in M3: without it, variant ids cannot be named
        value: Value,
        arms: Vec<(VariantId, BlockId)>,
        else_: BlockId,
    },
    Yield(YieldSite),             // suspends to host, resumes at YieldSite::resume
    Unreachable,
}

pub enum StmtKind {
    Assign { dst: Place, op: BinOp, a: Value, b: Value },
    AssignUn { dst: Place, op: UnOp, a: Value },
    Load { dst: Place, src: Operand },

    // Added in M3: `Callee` may be indirect, so a lambda held in a variable is callable —
    // which the grammar allows and `func: FuncRef` could not express.
    Call { dst: Option<Place>, callee: Callee, args: Vec<Value> },

    Cmd { kind: CommandKind, args: Vec<Value> },   // builds a presentation command

    // Added in M3: the first draft could not *construct* any aggregate, so a program
    // could not create a list, a map, a struct, or a variant.
    ListNew { dst: Place, items: Vec<Value> },
    MapNew { dst: Place, entries: Vec<(Value, Value)> },
    StructNew { dst: Place, name: String, fields: Vec<(String, Value)> },
    EnumNew { dst: Place, enum_name: String, variant: String, args: Vec<Value> },
    EnumField { dst: Place, base: Value, index: u32 },   // a matched variant's payload

    // Added in M3: `??` is in the language, so a coalesce needed a lowering.
    IsNone { dst: Place, base: Value },
    Unwrap { dst: Place, base: Value },
}
```

`Place` is where a value can be written (`Local`, `Default`, `Field`, `Index`); `Operand` is
where one can be read from (`Value`, `Default`, `Field`, `Index`, `Len`). They are different
questions, and the two `Default` cases exist because a `default` is a *place*, not a value —
reading one is its own operand rather than a value that happens to be elsewhere.

Fields are named in MIR and indexed in bytecode. A disassembly that says `route.name` is
worth more than one that says `route.3`, and resolving the name to an index is codegen's job,
where the instruction set demands one.

`Value` is `Slot(u32) | Const(ConstId)`. Every `Value` has a known `Ty` at construction.

### 2.1 Optimization passes (M4)

The pass pipeline is a registry (`CONVENTIONS.md §4.5`). Initial set, in order:

| Pass | What it does | Why it matters here |
| --- | --- | --- |
| `const_fold` | Evaluate constant expressions | Removes `str` interpolation of constants |
| `branch_simplify` | Fold constant branches | Makes unreachable-code diagnostics precise |
| `dead_block` | Remove blocks with no predecessors | Small win; keeps disassembly readable |
| `inline_small` | Inline `fn` bodies under a size threshold | Avoids call overhead in per-frame logic |
| `cmd_fuse` | Drop a `Cmd` the next one overwrites | One yield site per command |

> **Corrected during M3.**  The first draft listed `dead_block` *before* `branch_simplify`,
> which is backwards: folding a decided branch is exactly what *creates* unreachable blocks.
> Running the cleanup first meant the blocks it exists to remove were still there afterwards,
> and rule 8 of §4 ("unreachable code must be removed, not merely present") failed on any
> story with a constant condition.

`cmd_fuse` is worth a note of its own. Lowering emits exactly one `Cmd` per block by
construction, so on lowering's output the pass has nothing to do. It is there to *maintain*
that invariant: if a later pass ever spliced a second command into a block, this is what
removes it. The harness asserts the invariant directly rather than assuming the pass is
exercised.

Passes must be **observably semantics-preserving** and are covered by a differential test:
run the test corpus unoptimized and optimized, assert identical command streams and final
`World` regardless of opt level.

## 3. Bytecode module

### 3.1 Encoding

Little-endian, fixed-width, no alignment padding.

```rust
pub struct Module {
    pub header: Header,
    pub strings: StringTable,
    pub consts: ConstPool,
    pub types: TypeTable,
    pub schema: SchemaHash,
    pub fns: Vec<FuncDef>,
    pub labels: Vec<LabelDef>,
    pub defaults: Vec<DefaultDef>,
    pub cmds: Vec<CmdSchema>,
    pub debug: DebugInfo,
    pub checksum: u64,
}

pub struct Header {
    pub magic: [u8; 4],        // b"VELA"
    pub format: u16,           // bytecode format version
    pub abi: u16,              // plugin ABI version the module was built against
    pub flags: u32,            // bit 0: debug info present, bit 1: has source map
    pub schema_digest: [u8; 32],
}
```

`format` is bumped only for **incompatible** bytecode changes; the loader rejects unknown
formats with `E7101` and a clear "rebuild required" message. Additive changes to command
schemas or the type table do **not** bump it.

### 3.2 Instruction set

Every instruction is two bytes: opcode + operand kind. Operands are variable-width and
little-endian. The table below is normative and lives in `vela-bytecode/src/op.rs` as data —
the interpreter and verifier both read it, so they cannot disagree.

| Category | Ops | Stack effect |
| --- | --- | --- |
| Constants | `ConstI i64`, `ConstF f64`, `ConstS str`, `ConstB bool`, `ConstNone` | `[] → [T]` |
| Slots | `LoadLocal u32`, `StoreLocal u32` | `[] → [T]` / `[T] → []` |
| World | `LoadDefault u32`, `StoreDefault u32` | `[] → [T]` / `[T] → []` |
| Aggregates | `ListNew n`, `ListGet`, `ListSet`, `ListLen`, `MapNew n`, `MapSet`, `MapGet`, `MapHas`, `StructNew struct_id`, `FieldGet field`, `FieldSet field`, `EnumNew variant`, `EnumTag`, `EnumField field` | varies (declared per op) |
| Arithmetic | `AddI SubI MulI DivI ModI NegI`, `AddF SubF MulF DivF NegF` | `[T,T] → [T]` |
| Compare | `EqI EqF EqS EqB`, `LtI LtF LeI LeF GtI GtF GeI GeF` | `[T,T] → [bool]` |
| Logic | `Not` | `[bool] → [bool]` |
| Control | `Jump off`, `JumpIfFalse off`, `JumpIfTrue off`, `Return n`, `CallFn fn_id`, `CallLabel label_id` | varies |
| Match | `Dispatch n` (pops a tag, jumps via jump table) | `[tag] → []` |
| Optionals | `IsNone`, `UnwrapOr` | `[T?] → [bool]` / `[T?,T] → [T]` |
| Strings | `Concat n`, `ToStr`, `ToInt`, `ToFloat` | varies |
| Commands | `Cmd variant_id n` | `[args…] → [cmd]` |
| Effects | `CallEffect effect_id n` | `[args…] → [T]` |
| Suspend | `Yield` | `[cmd] → [result]` |
| Stack | `Dup`, `Pop`, `Nop` | `[T] → [T,T]` / `[T] → []` |

Short-circuit `and`/`or` are lowered to branches, not instructions. Guards and `if` become
`JumpIfFalse`. This keeps the opcode set small enough to verify exhaustively.

### 3.3 Commands vs. effects

Two distinct ways bytecode talks to the outside world, and the distinction is load-bearing:

- **Command** (`Cmd` + `Yield`): a *presentation* directive — `Say`, `Show`, `Hide`,
  `Scene`, `Play`, `Stop`, `Wait`, `Pause`. Commands are declarative and **the VM never sees
  their result except an acknowledgement**. They are how the story tells the UI what to
  display.
- **Effect** (`CallEffect`): a *capability* call that returns a value — `input.choose`,
  `fs.read`, `rand`, `now`, `audio.position`. Effects declare required capabilities
  (`ARCHITECTURE.md §6.3`) and their results are recorded in the input log.

**Adding a command or effect is a schema registration, not an instruction.** This narrows the
"adding an instruction" exception in `CONVENTIONS.md §4.6` to genuinely new *arithmetic or
control* operations, which are rare.

Command variants are declared in `Module.cmds` as a name plus a field schema, so the loader
and the UI both read the same description. Unknown variant → `E7102`, and old runtimes can
correctly reject a module that uses a command they do not know.

## 4. Verifier

Runs on load (and in CI over the golden corpus). Any failure is `E6xxx` — always a compiler
bug, so the report includes the MIR and bytecode in the error.

Rules:

1. **Stack depth** — for every reachable block, incoming stack depth is identical on all
   paths; simulated stack never underflows the frame base.
2. **Stack types** — each op's operands match its declared stack effect; the type table is
   consulted for aggregates.
3. **Jump targets** — every branch/table target is a valid instruction boundary.
4. **Locals** — a `LoadLocal` is dominated by a `StoreLocal` on every path (or the slot is a
   parameter).
5. **Terminators** — every reachable block ends in a terminator; no fallthrough past the last
   instruction; `Return` arity matches the body's declared return type.
6. **Match totality** — `Dispatch` jump tables are dense and cover the enum's declared
   variant range (a hole means a non-exhaustive match slipped through, `E4001` again).
   Checking this is why `Dispatch` names its enum: a table of `VariantId`s with no enum is
   not something a verifier — or a reader — can check against anything.
7. **Command/effect arity** — operand count matches the registered schema.
8. **No orphan code** — unreachable blocks must be removed by `dead_block`, not merely
   present, so the verifier's model is total.

## 5. Debug info

When compiled with `--debug` (the default for `vela run` and `vela test`), the module carries:

- A **span table** mapping instruction offsets → source `Span`.
- **Slot names** mapping `u32` → original identifier, for the debugger's variable view.
- **Line tables** for stepping, consumed by the DAP server (`TOOLING.md §6`).

Release builds drop debug info to shrink the bundle; `vela build --release` sets
`Header::flags` bit 0 to 0. The span table can also be shipped as a **separate sidecar**
(`game.velac.debug`) so a release build can still be symbolicated for crash reports without
paying the size cost in the shipped artifact.

## 6. Versioning policy

| Change | Action |
| --- | --- |
| New command/effect variant | Add to schema. No format bump. Old runtimes reject with `E7102`. |
| New instruction | Bump `format`. Old loader rejects cleanly. |
| Change to MIR | Internal — no format bump, but all golden disassembly files are re-blessed in one commit. |
| Change to plugin ABI | Bump `abi`; see `check-abi`. |

Forward compatibility is a non-goal. Backward compatibility (a new runtime running an old
module) is required and tested: `tests/golden/velac/` keeps one module per historical format
and CI loads each one.

## 7. Golden corpus

`tests/golden/bytecode/` holds, per construct, the source and its `.expected` disassembly.
`cargo xtask bless` regenerates them; CI fails on unblessed drift. This corpus is the
regression net for the entire back end — if a pass breaks semantics, the differential test
(`§2.1`) catches behavior and the golden diff shows exactly which instruction changed.
