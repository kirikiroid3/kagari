//! Expressions as places.

use crate::mir::{Place, PlaceBase, Proj};
use crate::thir::{ThirExprId, ThirExprKind};

use super::{BasicBlock, BlockAnd, Builder};

impl Builder<'_> {
    /// The place `expr` denotes: locals, story variables and projections of places; any
    /// other expression is evaluated into a temporary.
    pub(super) fn as_place(&mut self, block: BasicBlock, expr: ThirExprId) -> BlockAnd<Place> {
        stacker::maybe_grow(crate::STACK_RED_ZONE, crate::STACK_GROWTH, || {
            self.with_src(expr, |this| this.as_place_inner(block, expr))
        })
    }

    fn as_place_inner(&mut self, mut block: BasicBlock, expr: ThirExprId) -> BlockAnd<Place> {
        let thir = self.thir;
        let place = match &thir.expr(expr).kind {
            ThirExprKind::Local(id) => Place::local(self.local(*id)),
            ThirExprKind::StoryVar(item) => Place {
                base: PlaceBase::StoryVar(*item),
                proj: Default::default(),
            },
            ThirExprKind::Field { base, index } => {
                let base = unpack!(block = self.as_place(block, *base));
                base.project(Proj::Field(*index))
            }
            ThirExprKind::Index { base, index } => {
                let base = unpack!(block = self.as_place(block, *base));
                let index = match thir.expr(*index).kind {
                    ThirExprKind::Local(id) => self.local(id),
                    _ => unpack!(block = self.as_temp(block, *index)),
                };
                base.project(Proj::Index(index))
            }
            ThirExprKind::Deref(inner) => {
                let inner = unpack!(block = self.as_place(block, *inner));
                inner.project(Proj::Deref)
            }
            _ => Place::local(unpack!(block = self.as_temp(block, expr))),
        };
        BlockAnd(block, place)
    }
}
