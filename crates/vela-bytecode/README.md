# vela-bytecode

The instruction set, assembler/disassembler, verifier, and the `.velac` codec.

**Owns:** The OpSpec table, codegen from MIR, verification, container read/write.

**Does not own:** Execution (vela-vm); optimization (vela-mir).

Rank `5`. See `docs/ARCHITECTURE.md §1`.
