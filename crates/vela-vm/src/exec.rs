//! The dispatch over data instructions.
//!
//! Every instruction that is not control flow arrives here, and is sent on by *family*: what
//! moves a value onto the stack, what computes from it, and what builds an aggregate are
//! three different jobs that happen to share an opcode space. The dispatch is a list of
//! opcodes; the work is in `ops`.

use vela_bytecode::{Op, Operand};
use vela_world::World;

use crate::fault::Fault;
use crate::machine::Vm;
use crate::ops;

/// Executes one data instruction.
pub(crate) fn simple(
    vm: &mut Vm,
    op: Op,
    operand: &Operand,
    world: &mut World,
) -> Result<(), Fault> {
    match op {
        Op::Nop
        | Op::Dup
        | Op::Pop
        | Op::ConstI
        | Op::ConstF
        | Op::ConstB
        | Op::ConstNone
        | Op::ConstS
        | Op::ConstFn
        | Op::LoadLocal
        | Op::StoreLocal
        | Op::LoadDefault
        | Op::StoreDefault
        | Op::CallEffect => ops::storage(vm, op, operand, world),
        Op::AddI
        | Op::SubI
        | Op::MulI
        | Op::DivI
        | Op::ModI
        | Op::NegI
        | Op::AddF
        | Op::SubF
        | Op::MulF
        | Op::DivF
        | Op::NegF
        | Op::EqI
        | Op::EqF
        | Op::EqS
        | Op::EqB
        | Op::LtI
        | Op::LtF
        | Op::LtS
        | Op::LeI
        | Op::LeF
        | Op::LeS
        | Op::GtI
        | Op::GtF
        | Op::GtS
        | Op::GeI
        | Op::GeF
        | Op::GeS
        | Op::Not
        | Op::Concat
        | Op::ToStr
        | Op::ToInt
        | Op::ToFloat
        | Op::ToBool
        | Op::IsNone
        | Op::Unwrap
        | Op::UnwrapOr => ops::compute(vm, op, operand),
        Op::ListNew
        | Op::MapNew
        | Op::ListGet
        | Op::MapGet
        | Op::MapHas
        | Op::ListSet
        | Op::MapSet
        | Op::ListLen
        | Op::StructNew
        | Op::FieldGet
        | Op::FieldSet
        | Op::EnumNew
        | Op::EnumTag
        | Op::EnumField
        | Op::Cmd => ops::aggregate(vm, op, operand),
        other => Err(Fault::BadSchema(format!(
            "{other:?} is not a data instruction"
        ))),
    }?;
    Ok(())
}
