//! The instruction set, assembler/disassembler, verifier, and the `.velac` codec.
//!
//! # Owns
//!
//! The opcode table, the `Module` a compiled story is, the verifier's eight rules, and the
//! container format.
//!
//! # Does not own
//!
//! What the instructions *mean* at run time (`vela-vm`, M5) or what the compiler decides
//! (`vela-mir`). This crate is the contract between them: a module that verifies is one
//! whose meaning is defined.
//!
//! # The instruction table is data
//!
//! `BYTECODE.md §3.2` calls the table normative, and the reason is structural: the
//! interpreter and the verifier both read it, so they cannot disagree about what an
//! instruction does. Two match statements would agree today and drift the first time one
//! was edited — and the drift would be a module that verifies and then does something else.
//!
//! # Work in progress
//!
//! M4 is being built in slices. Here and complete:
//!
//! - `op` — the instruction table, with operand shapes and stack effects
//! - `module` — `Module`, `Header`, the pools, command schemas, and debug info
//! - `asm` — MIR to bytecode
//! - `verify` — the eight rules of `BYTECODE.md §4`
//! - `disasm` — a listing driven by the same table the verifier reads
//! - `error` — the `E6xxx` and `E7xxx` diagnostics
//!
//! - `codec` — the `.velac` container, read and written

mod asm;
mod codec;
mod disasm;
pub mod error;
mod module;
mod op;
mod verify;

pub use asm::compile;
pub use codec::{DecodeError, decode, encode, known_commands};
pub use disasm::{disassemble, disassemble_commands};
pub use module::{
    ABI, ByteConst, ByteTy, CommandSchema, ConstId, ConstPool, DebugInfo, DefaultDef, EffectSchema,
    EnumDef, FORMAT, FieldSchema, FuncDef, Header, Instr, LocalDef, Module, StringId, StringTable,
    StructDef, TypeId, TypeTable, VariantDef,
};
pub use op::{Effect, NO_ENUM, OPS, Op, OpSpec, Operand, OperandKind};
pub use verify::verify;

#[cfg(test)]
mod tests;
