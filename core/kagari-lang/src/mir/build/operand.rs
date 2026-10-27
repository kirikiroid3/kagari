//! Expressions as operands and temporaries, and the categories that decide how an
//! expression is lowered.

use smallvec::SmallVec;

use crate::mir::{ConstOperand, Local, Operand, Place};
use crate::thir::{ThirExprId, ThirExprKind};

use super::{BasicBlock, BlockAnd, Builder};

/// How an expression is lowered.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Category {
    /// A constant operand.
    Constant,
    /// A place.
    Place,
    /// An rvalue computed from operands.
    Rvalue,
    /// Control flow, a call or an assignment: written into a destination.
    Into,
}

pub(super) fn category(kind: &ThirExprKind) -> Category {
    match kind {
        ThirExprKind::Literal(_)
        | ThirExprKind::ConstItem { .. }
        | ThirExprKind::ConstParam { .. }
        | ThirExprKind::FnItem(_)
        | ThirExprKind::Handle(_)
        | ThirExprKind::StaticValue(_)
        | ThirExprKind::Promoted(_) => Category::Constant,
        ThirExprKind::Local(_)
        | ThirExprKind::StoryVar(_)
        | ThirExprKind::Field { .. }
        | ThirExprKind::Index { .. }
        | ThirExprKind::Deref(_) => Category::Place,
        ThirExprKind::Unary { .. }
        | ThirExprKind::Binary { .. }
        | ThirExprKind::Tuple(_)
        | ThirExprKind::Array(_)
        | ThirExprKind::Repeat { .. }
        | ThirExprKind::Adt { .. }
        | ThirExprKind::Closure { .. }
        | ThirExprKind::Cast { .. }
        | ThirExprKind::Coerce { .. }
        | ThirExprKind::Borrow { .. } => Category::Rvalue,
        ThirExprKind::Block { .. }
        | ThirExprKind::If { .. }
        | ThirExprKind::Match { .. }
        | ThirExprKind::Loop { .. }
        | ThirExprKind::While { .. }
        | ThirExprKind::For { .. }
        | ThirExprKind::Break { .. }
        | ThirExprKind::Continue { .. }
        | ThirExprKind::Return { .. }
        | ThirExprKind::Call { .. }
        | ThirExprKind::Logical { .. }
        | ThirExprKind::Assign { .. }
        | ThirExprKind::AssignOp { .. }
        | ThirExprKind::Try { .. }
        | ThirExprKind::Error => Category::Into,
    }
}

impl Builder<'_> {
    /// `expr` as an operand: constants directly, places as copies (read where the operand
    /// is used), anything else through a temporary (of the coerced type). Coercions
    /// preserve the value, so a coerced constant or place is used as is.
    pub(super) fn as_operand(&mut self, block: BasicBlock, expr: ThirExprId) -> BlockAnd<Operand> {
        let inner = self.uncoerced(expr);
        let thir = self.thir;
        match category(&thir.expr(inner).kind) {
            Category::Constant => BlockAnd(block, self.constant(inner)),
            Category::Place => {
                let mut block = block;
                let place = unpack!(block = self.as_place(block, inner));
                BlockAnd(block, Operand::Copy(place))
            }
            Category::Rvalue | Category::Into => {
                let mut block = block;
                let temp = unpack!(block = self.as_temp(block, expr));
                BlockAnd(block, Operand::Copy(Place::local(temp)))
            }
        }
    }

    /// `expr` without its coercions.
    pub(super) fn uncoerced(&self, mut expr: ThirExprId) -> ThirExprId {
        while let ThirExprKind::Coerce { expr: inner, .. } = self.thir.expr(expr).kind {
            expr = inner;
        }
        expr
    }

    /// `expr` as an operand whose value is fixed now: a place is copied to a temporary,
    /// for operands followed by code that may write to it.
    pub(super) fn as_stable_operand(
        &mut self,
        block: BasicBlock,
        expr: ThirExprId,
    ) -> BlockAnd<Operand> {
        let inner = self.uncoerced(expr);
        if category(&self.thir.expr(inner).kind) == Category::Place {
            let mut block = block;
            let temp = unpack!(block = self.as_temp(block, expr));
            return BlockAnd(block, Operand::Copy(Place::local(temp)));
        }
        self.as_operand(block, expr)
    }

    /// `exprs` as operands, evaluated left to right: an operand followed by an expression
    /// with effects is copied before it.
    pub(super) fn as_operands(
        &mut self,
        block: BasicBlock,
        exprs: &[ThirExprId],
    ) -> BlockAnd<SmallVec<[Operand; 4]>> {
        // `effects_after[i]`: whether an expression after the `i`-th may have effects.
        let mut effects_after = vec![false; exprs.len()];
        let mut seen = false;
        for (index, &expr) in exprs.iter().enumerate().rev() {
            if let Some(slot) = effects_after.get_mut(index) {
                *slot = seen;
            }
            seen = seen || !self.is_pure(expr);
        }
        let mut block = block;
        let mut ops = SmallVec::with_capacity(exprs.len());
        for (&expr, &stable) in exprs.iter().zip(&effects_after) {
            let op = if stable {
                unpack!(block = self.as_stable_operand(block, expr))
            } else {
                unpack!(block = self.as_operand(block, expr))
            };
            ops.push(op);
        }
        BlockAnd(block, ops)
    }

    /// `expr` evaluated into a fresh temporary.
    pub(super) fn as_temp(&mut self, block: BasicBlock, expr: ThirExprId) -> BlockAnd<Local> {
        let temp = self.temp(self.thir.expr(expr).ty.clone());
        let mut block = block;
        unpack!(block = self.expr_into_dest(Some(Place::local(temp)), block, expr));
        BlockAnd(block, temp)
    }

    /// The constant operand of a `Category::Constant` expression.
    fn constant(&mut self, expr: ThirExprId) -> Operand {
        let constant = match &self.thir.expr(expr).kind {
            ThirExprKind::Literal(value) => ConstOperand::Value(value.clone()),
            ThirExprKind::ConstItem { item, args } => ConstOperand::Item {
                item: *item,
                args: args.clone(),
            },
            ThirExprKind::ConstParam { owner, index } => ConstOperand::Param {
                owner: *owner,
                index: *index,
            },
            ThirExprKind::FnItem(fn_ref) => ConstOperand::Fn(fn_ref.clone()),
            ThirExprKind::Handle(item) => ConstOperand::Handle(*item),
            ThirExprKind::StaticValue(item) => ConstOperand::Static(*item),
            ThirExprKind::Promoted(id) => ConstOperand::Promoted(*id),
            _ => ConstOperand::Value(crate::eval::ConstValue::unit()),
        };
        Operand::Const(constant)
    }

    /// Whether evaluating `expr` cannot write to any place: it only reads, computes and
    /// borrows.
    pub(super) fn is_pure(&self, expr: ThirExprId) -> bool {
        stacker::maybe_grow(crate::STACK_RED_ZONE, crate::STACK_GROWTH, || {
            let thir = self.thir;
            match &thir.expr(expr).kind {
                ThirExprKind::Literal(_)
                | ThirExprKind::ConstItem { .. }
                | ThirExprKind::ConstParam { .. }
                | ThirExprKind::FnItem(_)
                | ThirExprKind::Handle(_)
                | ThirExprKind::StaticValue(_)
                | ThirExprKind::StoryVar(_)
                | ThirExprKind::Local(_)
                | ThirExprKind::Promoted(_)
                | ThirExprKind::Closure { .. } => true,
                ThirExprKind::Field { base: inner, .. }
                | ThirExprKind::Deref(inner)
                | ThirExprKind::Borrow { place: inner, .. }
                | ThirExprKind::Unary { expr: inner, .. }
                | ThirExprKind::Repeat { value: inner, .. }
                | ThirExprKind::Cast { expr: inner, .. }
                | ThirExprKind::Coerce { expr: inner, .. } => self.is_pure(*inner),
                ThirExprKind::Index {
                    base: lhs,
                    index: rhs,
                }
                | ThirExprKind::Binary { lhs, rhs, .. }
                | ThirExprKind::Logical { lhs, rhs, .. } => {
                    self.is_pure(*lhs) && self.is_pure(*rhs)
                }
                ThirExprKind::Tuple(items) | ThirExprKind::Array(items) => {
                    items.iter().all(|item| self.is_pure(*item))
                }
                ThirExprKind::Adt { fields, .. } => {
                    fields.iter().all(|(_, field)| self.is_pure(*field))
                }
                _ => false,
            }
        })
    }
}
