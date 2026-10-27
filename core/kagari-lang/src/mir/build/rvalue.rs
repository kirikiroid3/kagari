//! Expressions as rvalues: arithmetic chosen by operand type, comparisons, aggregates,
//! borrows, closures and casts. Coercions preserve the value at run time.

use smallvec::SmallVec;

use crate::hir::{BinaryOp, UnaryOp};
use crate::mir::{AggregateKind, Operand, Place, Rvalue};
use crate::thir::{ThirExprId, ThirExprKind};
use crate::ty::{Ty, TyKind};

use super::operand::{Category, category};
use super::{BasicBlock, BlockAnd, Builder};

/// `ty` without its references.
pub(super) fn strip_refs(ty: &Ty) -> Ty {
    let mut current = ty;
    while let TyKind::Ref { inner, .. } = current.kind() {
        current = inner;
    }
    current.clone()
}

pub(super) fn is_comparison(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge
    )
}

impl Builder<'_> {
    pub(super) fn as_rvalue(&mut self, block: BasicBlock, expr: ThirExprId) -> BlockAnd<Rvalue> {
        stacker::maybe_grow(crate::STACK_RED_ZONE, crate::STACK_GROWTH, || {
            self.with_src(expr, |this| this.as_rvalue_inner(block, expr))
        })
    }

    fn as_rvalue_inner(&mut self, mut block: BasicBlock, expr: ThirExprId) -> BlockAnd<Rvalue> {
        let thir = self.thir;
        let node = thir.expr(expr);
        let rvalue = match &node.kind {
            ThirExprKind::Unary { op, expr: inner } => {
                let operand = unpack!(block = self.as_operand(block, *inner));
                self.unary_rvalue(*op, operand, &thir.expr(*inner).ty, &node.ty)
            }
            ThirExprKind::Binary { op, lhs, rhs } => {
                let ops = unpack!(block = self.as_operands(block, &[*lhs, *rhs]));
                let mut ops = ops.into_iter();
                let (Some(left), Some(right)) = (ops.next(), ops.next()) else {
                    return self.abort_rvalue(block);
                };
                self.binary_rvalue(*op, left, right, &thir.expr(*lhs).ty, &node.ty)
            }
            ThirExprKind::Tuple(items) => Rvalue::Aggregate {
                kind: AggregateKind::Tuple,
                ops: unpack!(block = self.as_operands(block, items)),
            },
            ThirExprKind::Array(items) => Rvalue::Aggregate {
                kind: AggregateKind::Array,
                ops: unpack!(block = self.as_operands(block, items)),
            },
            ThirExprKind::Repeat { value, count } => Rvalue::Repeat {
                operand: unpack!(block = self.as_operand(block, *value)),
                count: *count,
            },
            ThirExprKind::Adt { ctor, fields } => {
                // Evaluated in source order, stored in field order.
                let exprs: Vec<ThirExprId> = fields.iter().map(|(_, value)| *value).collect();
                let values = unpack!(block = self.as_operands(block, &exprs));
                let count = self
                    .cx
                    .sigs
                    .adt(ctor.adt)
                    .and_then(|adt| adt.variants.get(ctor.variant.unwrap_or(0) as usize))
                    .map_or(0, |variant| variant.fields.len());
                let mut slots: Vec<Option<Operand>> = vec![None; count];
                for ((index, _), value) in fields.iter().zip(values) {
                    if let Some(slot) = slots.get_mut(*index as usize) {
                        *slot = Some(value);
                    }
                }
                let Some(ops) = slots.into_iter().collect::<Option<SmallVec<_>>>() else {
                    return self.abort_rvalue(block);
                };
                Rvalue::Aggregate {
                    kind: AggregateKind::Adt(ctor.clone()),
                    ops,
                }
            }
            ThirExprKind::Closure { index } => {
                let Some(closure) = thir.closure(*index) else {
                    return self.abort_rvalue(block);
                };
                let captures = closure
                    .captures
                    .iter()
                    .map(|local| Operand::Copy(Place::local(self.local(*local))))
                    .collect();
                Rvalue::Closure {
                    expr: closure.src,
                    captures,
                }
            }
            ThirExprKind::Cast { expr: inner, ty } => Rvalue::Cast {
                operand: unpack!(block = self.as_operand(block, *inner)),
                ty: ty.clone(),
            },
            ThirExprKind::Coerce { expr: inner, .. } => {
                // Coercions preserve the value: an rvalue is computed as is.
                if category(&thir.expr(self.uncoerced(*inner)).kind) == Category::Rvalue {
                    return self.as_rvalue(block, *inner);
                }
                Rvalue::Use(unpack!(block = self.as_operand(block, expr)))
            }
            ThirExprKind::Borrow { mutable, place } => Rvalue::Ref {
                mutable: *mutable,
                place: unpack!(block = self.as_place(block, *place)),
            },
            kind => match category(kind) {
                Category::Constant | Category::Place => {
                    Rvalue::Use(unpack!(block = self.as_operand(block, expr)))
                }
                Category::Rvalue | Category::Into => {
                    let temp = unpack!(block = self.as_temp(block, expr));
                    Rvalue::Use(Operand::Copy(Place::local(temp)))
                }
            },
        };
        BlockAnd(block, rvalue)
    }

    /// Ends `block` as unreachable; the rvalue computed in the dead block after it is `()`.
    fn abort_rvalue(&mut self, block: BasicBlock) -> BlockAnd<Rvalue> {
        let BlockAnd(block, ()) = self.abort(block);
        BlockAnd(block, Self::unit())
    }

    fn is_std_adt(&self, ty: &Ty, item: Option<crate::item_tree::ItemId>) -> bool {
        matches!(ty.kind(), TyKind::Adt { item: adt, .. } if Some(*adt) == item)
    }

    /// `-` or `!` on an operand of type `operand_ty`, giving `result_ty`.
    pub(super) fn unary_rvalue(
        &self,
        op: UnaryOp,
        operand: Operand,
        operand_ty: &Ty,
        result_ty: &Ty,
    ) -> Rvalue {
        let operand_ty = strip_refs(operand_ty);
        let std = &self.cx.sigs.std;
        match op {
            UnaryOp::Not if matches!(operand_ty.kind(), TyKind::Bool) => Rvalue::BoolNot(operand),
            UnaryOp::Not => Rvalue::IntNot {
                operand,
                ty: strip_refs(result_ty),
            },
            UnaryOp::Neg if self.is_std_adt(&operand_ty, std.decimal) => Rvalue::DecNeg(operand),
            UnaryOp::Neg if self.is_std_adt(&operand_ty, std.rat) => Rvalue::RatNeg(operand),
            UnaryOp::Neg => Rvalue::IntNeg {
                operand,
                ty: strip_refs(result_ty),
            },
            // `*` is a place projection, never an rvalue.
            UnaryOp::Deref => Rvalue::Use(operand),
        }
    }

    /// The binary operator `op` (not `&&`/`||`) on operands of type `operand_ty`, giving
    /// `result_ty`.
    pub(super) fn binary_rvalue(
        &self,
        op: BinaryOp,
        lhs: Operand,
        rhs: Operand,
        operand_ty: &Ty,
        result_ty: &Ty,
    ) -> Rvalue {
        if is_comparison(op) {
            return Rvalue::Cmp { op, lhs, rhs };
        }
        let operand_ty = strip_refs(operand_ty);
        let std = &self.cx.sigs.std;
        if matches!(operand_ty.kind(), TyKind::Bool) {
            Rvalue::BoolBin { op, lhs, rhs }
        } else if self.is_std_adt(&operand_ty, std.decimal) {
            Rvalue::DecBin { op, lhs, rhs }
        } else if self.is_std_adt(&operand_ty, std.rat) {
            Rvalue::RatBin { op, lhs, rhs }
        } else {
            Rvalue::IntBin {
                op,
                lhs,
                rhs,
                ty: strip_refs(result_ty),
            }
        }
    }
}
