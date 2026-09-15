//! Emitting values, places, and operands.

use vela_mir::{Operand as MirOperand, Place, Value as MirValue};
use vela_types::Ty;

use crate::op::Op;
use crate::op::Operand;

use super::body::Emitter;

impl Emitter<'_> {
    /// Pushes a value onto the stack.
    pub(super) fn value(&mut self, value: MirValue) {
        match value {
            MirValue::Slot(slot) => self.emit(Op::LoadLocal, Operand::U32(slot.0)),
            MirValue::Const(id) => {
                let constant = self.builder.constant(self.module, id);
                self.constant(constant);
            }
        }
    }

    /// Pushes a constant.
    fn constant(&mut self, constant: crate::module::ByteConst) {
        use crate::module::ByteConst;

        match constant {
            ByteConst::None => self.emit(Op::ConstNone, Operand::None),
            ByteConst::Bool(flag) => {
                self.emit(Op::ConstB, Operand::U32(u32::from(flag)));
            }
            ByteConst::Int(number) => self.emit(Op::ConstI, Operand::I64(number)),
            ByteConst::Float(number) => self.emit(Op::ConstF, Operand::F64(number)),
            ByteConst::Str(id) => self.emit(Op::ConstS, Operand::Str(id.0)),
            // A variant with no payload is built rather than stored: it is one instruction
            // either way, and this keeps the constant pool to values that have no
            // construction of their own.
            //
            // The operands are `(enum, variant)`, the order `EnumNew` is read in everywhere else.
            // This wrote `(variant, 0)` and dropped the enum, so `Ending.cold` as a *value* built
            // whichever enum happened to sit at index `cold`'s position — a fault, or worse, a
            // value of the wrong type.
            ByteConst::Variant { enum_id, variant } => {
                self.emit(Op::EnumNew, Operand::Pair(enum_id, variant));
            }
            ByteConst::Function(index) => self.emit(Op::ConstFn, Operand::U32(index)),
        }
    }

    /// Pushes the current contents of a place.
    pub(super) fn place_object(&mut self, place: &Place) {
        match place {
            Place::Local(slot) => self.emit(Op::LoadLocal, Operand::U32(slot.0)),
            Place::Default(id) => self.emit(Op::LoadDefault, Operand::U32(id.0)),
            Place::Field { base, field } => {
                self.place_object(base);
                let name = self.builder.strings.add(field.clone());
                self.emit(Op::FieldGet, Operand::Str(name.0));
            }
            Place::Index { base, index } => {
                self.place_object(base);
                let index = *index;
                self.value(index);
                let op = self.index_op(self.place_ty(base));
                self.emit(op, Operand::None);
            }
        }
    }

    /// Stores into a place, with the value pushed by `value`.
    ///
    /// Recursive, and it has to be: an aggregate is a value, so `a.b[0] = v` is a *rebuild*
    /// of `a` with a rebuilt `b`. Each level pushes the object it is about to change, takes
    /// the updated one back, and hands it to the level above — which is why the store walks
    /// outward from the innermost place.
    pub(super) fn store(&mut self, place: &Place, value: &mut dyn FnMut(&mut Self)) {
        match place {
            Place::Local(slot) => {
                value(self);
                self.emit(Op::StoreLocal, Operand::U32(slot.0));
            }
            Place::Default(id) => {
                value(self);
                self.emit(Op::StoreDefault, Operand::U32(id.0));
            }
            Place::Field { base, field } => {
                let base = (**base).clone();
                let name = self.builder.strings.add(field.clone());
                self.store(&base, &mut |this| {
                    this.place_object(&base);
                    value(this);
                    this.emit(Op::FieldSet, Operand::Str(name.0));
                });
            }
            Place::Index { base, index } => {
                let base = (**base).clone();
                let index = *index;
                let op = self.set_op(self.place_ty(&base));
                self.store(&base, &mut |this| {
                    this.place_object(&base);
                    this.value(index);
                    value(this);
                    this.emit(op, Operand::None);
                });
            }
        }
    }

    /// Reads an operand into a place.
    pub(super) fn load(&mut self, dst: &Place, src: &MirOperand) {
        match src {
            MirOperand::Value(value) => {
                let value = *value;
                self.store(dst, &mut |this| this.value(value));
            }
            MirOperand::Default(id) => {
                let id = id.0;
                self.store(dst, &mut |this| {
                    this.emit(Op::LoadDefault, Operand::U32(id))
                });
            }
            MirOperand::Field { base, field } => {
                let base = *base;
                let name = self.builder.strings.add(field.clone());
                self.store(dst, &mut |this| {
                    this.value(base);
                    this.emit(Op::FieldGet, Operand::Str(name.0));
                });
            }
            MirOperand::Index { base, index } => {
                let (base, index) = (*base, *index);
                let op = self.index_op(self.ty_of(base));
                self.store(dst, &mut |this| {
                    this.value(base);
                    this.value(index);
                    this.emit(op, Operand::None);
                });
            }
            MirOperand::Len { base } => {
                let base = *base;
                self.store(dst, &mut |this| {
                    this.value(base);
                    this.emit(Op::ListLen, Operand::None);
                });
            }
        }
    }

    /// The instruction that indexes a value of this type.
    pub(super) fn index_op(&self, ty: Ty) -> Op {
        match ty {
            Ty::Map(_, _) => Op::MapGet,
            Ty::Str => Op::ListGet,
            _ => Op::ListGet,
        }
    }

    /// The instruction that writes into a value of this type.
    pub(super) fn set_op(&self, ty: Ty) -> Op {
        match ty {
            Ty::Map(_, _) => Op::MapSet,
            _ => Op::ListSet,
        }
    }

    /// The type a place holds.
    ///
    /// Needed because a write's instruction depends on the aggregate it writes into, and by
    /// the time codegen reaches `xs[0] = v` the aggregate is a place rather than a value.
    pub(super) fn place_ty(&self, place: &Place) -> Ty {
        match place {
            Place::Local(slot) => self.body.slot_ty(*slot).cloned().unwrap_or(Ty::Unknown),
            Place::Default(id) => self
                .module
                .defaults
                .get(id.0 as usize)
                .map_or(Ty::Unknown, |definition| definition.ty.clone()),
            Place::Field { base, field } => {
                let struct_name = match self.place_ty(base) {
                    Ty::Struct(name) => name,
                    _ => return Ty::Unknown,
                };
                self.module
                    .struct_named(&struct_name)
                    .and_then(|definition| {
                        definition
                            .fields
                            .iter()
                            .find(|candidate| candidate.name == *field)
                    })
                    .map_or(Ty::Unknown, |field| field.ty.clone())
            }
            Place::Index { base, .. } => match self.place_ty(base) {
                Ty::List(element) => *element,
                Ty::Map(_, value) => *value,
                _ => Ty::Unknown,
            },
        }
    }

    /// The type of a MIR value: the slot's declared type, or the constant's own.
    pub(super) fn ty_of(&self, value: MirValue) -> Ty {
        match value {
            MirValue::Slot(slot) => self.body.slot_ty(slot).cloned().unwrap_or(Ty::Unknown),
            MirValue::Const(id) => self.const_ty(id),
        }
    }

    /// The type of a pooled constant.
    ///
    /// Read from the pool rather than left `Unknown`, because the operator emitter *picks its
    /// instruction* from this type: `"i is " + text` compiled to `add.i` for as long as every
    /// literal was unknown, and `1.5 + x` to integer addition with it.
    fn const_ty(&self, id: vela_mir::ConstId) -> Ty {
        match self.module.pool.get(id) {
            Some(vela_mir::Const::None) => Ty::None,
            Some(vela_mir::Const::Bool(_)) => Ty::Bool,
            Some(vela_mir::Const::Int(_)) => Ty::Int,
            Some(vela_mir::Const::Float(_)) => Ty::Float,
            Some(vela_mir::Const::Str(_)) => Ty::Str,
            Some(vela_mir::Const::Variant { enum_name, .. }) => Ty::Enum(enum_name.clone()),
            // A function's type is a signature the value does not carry, and an id that is not in
            // the pool is a compiler bug the verifier reports; neither picks an operator.
            Some(vela_mir::Const::Function(_)) | None => Ty::Unknown,
        }
    }
}
