//! Emitting one MIR body as instructions.
//!
//! # Offsets are patched, not predicted
//!
//! A jump's target is a byte offset, and a byte offset is not known until the code before it
//! exists. So each body is emitted with placeholders and patched once every block has a
//! position. The alternative — a size pre-pass — has to agree with the emitter about the
//! width of every instruction, and the way to guarantee that two things agree is to have
//! only one of them.

use vela_mir::{Body as MirBody, Module as MirModule, VariantId};
use vela_span::Span;
use vela_syntax::{BinOp, UnOp};
use vela_types::Ty;

use crate::module::{FuncDef, Instr, LocalDef};
use crate::op::Op;
use crate::op::Operand;

use super::build::Builder;

/// A placeholder that a later pass fills in with a real byte offset.
pub(super) enum Fixup {
    /// The instruction's unsigned operand is a jump to a MIR block.
    Jump { at: usize, block: usize },
    /// The `entry`th target of the instruction's dispatch table is a MIR block.
    Table {
        at: usize,
        entry: usize,
        block: usize,
    },
}

/// Emits one body.
pub(super) struct Emitter<'a> {
    pub(super) builder: &'a mut Builder,
    pub(super) module: &'a MirModule,
    pub(super) body: &'a MirBody,
    pub(super) code: Vec<Instr>,
    pub(super) spans: Vec<Span>,
    /// Where each MIR block's first instruction is, by block index. `None` for a block
    /// nothing can reach, which is not emitted at all.
    block_at: Vec<Option<usize>>,
    pub(super) fixups: Vec<Fixup>,
    /// The span the instruction being emitted belongs to.
    pub(super) span: Span,
}

impl<'a> Emitter<'a> {
    /// Starts emitting a body.
    pub(super) fn new(builder: &'a mut Builder, module: &'a MirModule, body: &'a MirBody) -> Self {
        let blocks = body.blocks.len();
        Self {
            builder,
            module,
            body,
            code: Vec::new(),
            spans: Vec::new(),
            block_at: vec![None; blocks],
            fixups: Vec::new(),
            span: Span::new(vela_span::FileId::from_raw(0), 0, 0),
        }
    }

    /// Emits every reachable block and resolves the jumps.
    pub(super) fn run(&mut self) {
        // Unreachable blocks are skipped rather than emitted as dead code, because rule 8
        // of `BYTECODE.md §4` requires them to be *absent*. Doing it here rather than
        // relying on `dead_block` means a module built at `-O0` verifies too.
        let dead = self.body.unreachable_blocks();

        for block in &self.body.blocks {
            if dead.contains(&block.id) {
                continue;
            }
            self.block_at[block.id.0 as usize] = Some(self.code.len());

            for stmt in &block.stmts {
                self.statement(stmt);
            }
            self.span = self.span_of_terminator(block.id.0 as usize);
            self.terminator(&block.term);
        }

        self.patch();
    }

    /// The finished definition.
    pub(super) fn finish(self) -> FuncDef {
        let params = self.body.params.iter().map(|slot| slot.0).collect();
        let ret = self.builder.ty(&self.body.ret);
        let locals: Vec<LocalDef> = self
            .body
            .locals
            .iter()
            .map(|local| LocalDef {
                name: self.builder.strings.add(local.name.clone()),
                ty: self.builder.ty(&local.ty),
            })
            .collect();

        FuncDef {
            name: self.builder.strings.add(self.body.name.to_string()),
            params,
            ret,
            locals,
            code: self.code,
            spans: self.spans,
        }
    }

    /// Appends an instruction.
    pub(super) fn emit(&mut self, op: Op, operand: Operand) {
        self.code.push(Instr { op, operand });
        self.spans.push(self.span);
    }

    /// The span to attribute a block's terminator to.
    ///
    /// A terminator has no span of its own; the last statement in the block is where the
    /// author wrote whatever decided it, which is what a debugger should show.
    fn span_of_terminator(&self, index: usize) -> Span {
        self.body
            .blocks
            .get(index)
            .and_then(|block| block.stmts.last().map(|stmt| stmt.span))
            .unwrap_or(self.span)
    }

    /// Turns placeholders into byte offsets.
    pub(super) fn patch(&mut self) {
        let mut offsets: Vec<u32> = vec![0; self.block_at.len()];
        for (index, slot) in self.block_at.iter().enumerate() {
            if let Some(at) = slot {
                offsets[index] = self.offset_at(*at);
            }
        }

        for fixup in &self.fixups {
            match fixup {
                Fixup::Jump { at, block } => {
                    let target = offsets.get(*block).copied().unwrap_or(0);
                    if let Some(instr) = self.code.get_mut(*at) {
                        instr.operand = Operand::U32(target);
                    }
                }
                Fixup::Table { at, entry, block } => {
                    let target = offsets.get(*block).copied().unwrap_or(0);
                    if let Some(instr) = self.code.get_mut(*at)
                        && let Operand::Tables(_, targets) = &mut instr.operand
                        && let Some(slot) = targets.get_mut(*entry)
                    {
                        *slot = target;
                    }
                }
            }
        }
    }

    /// The byte offset of an instruction.
    pub(super) fn offset_at(&self, index: usize) -> u32 {
        let mut offset = 0u32;
        for instr in self.code.iter().take(index) {
            offset = offset.saturating_add(u32::try_from(instr.width()).unwrap_or(u32::MAX));
        }
        offset
    }

    /// Records that an instruction's operand is a jump to a MIR block.
    pub(super) fn jump_to(&mut self, op: Op, block: u32) {
        let at = self.code.len();
        self.emit(op, Operand::U32(0));
        self.fixups.push(Fixup::Jump {
            at,
            block: block as usize,
        });
    }
    /// A variant's position within its enum.
    pub(super) fn variant_index(&self, enum_name: &str, variant: &str) -> u32 {
        self.module
            .enum_named(enum_name)
            .and_then(|definition| definition.index_of(variant))
            .map_or(u32::MAX, |VariantId(index)| index)
    }

    /// The instruction for a binary operator over a type.
    pub(super) fn binary_op(&self, op: BinOp, ty: Ty) -> Op {
        use BinOp::{Add, Div, Eq, Ge, Gt, Le, Lt, Mul, Ne, Rem, Sub};

        let float = matches!(ty, Ty::Float);
        let text = matches!(ty, Ty::Str);

        match op {
            Add if text => Op::Concat,
            Add => {
                if float {
                    Op::AddF
                } else {
                    Op::AddI
                }
            }
            Sub => {
                if float {
                    Op::SubF
                } else {
                    Op::SubI
                }
            }
            Mul => {
                if float {
                    Op::MulF
                } else {
                    Op::MulI
                }
            }
            Div => {
                if float {
                    Op::DivF
                } else {
                    Op::DivI
                }
            }
            Rem => Op::ModI,
            Eq | Ne => {
                if text {
                    Op::EqS
                } else if float {
                    Op::EqF
                } else if matches!(ty, Ty::Bool) {
                    Op::EqB
                } else {
                    Op::EqI
                }
            }
            Lt | Le | Gt | Ge => match (float, text) {
                (true, _) => match op {
                    Lt => Op::LtF,
                    Le => Op::LeF,
                    Gt => Op::GtF,
                    _ => Op::GeF,
                },
                (false, true) => match op {
                    Lt => Op::LtS,
                    Le => Op::LeS,
                    Gt => Op::GtS,
                    _ => Op::GeS,
                },
                (false, false) => match op {
                    Lt => Op::LtI,
                    Le => Op::LeI,
                    Gt => Op::GtI,
                    _ => Op::GeI,
                },
            },
            // `and`, `or`, and `??` are lowered to branches by MIR, so they never reach
            // codegen. `is`, `in` and their negations are the same.
            _ => Op::Nop,
        }
    }

    /// The instruction for a prefix operator over a type.
    pub(super) fn unary_op(&self, op: UnOp, ty: Ty) -> Op {
        match op {
            UnOp::Neg => {
                if matches!(ty, Ty::Float) {
                    Op::NegF
                } else {
                    Op::NegI
                }
            }
            UnOp::Not => Op::Not,
        }
    }
}
