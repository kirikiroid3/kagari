//! The bodies of an item: its own, its closures' (captures first among the arguments) and
//! its promoted constants' (a prelude binding the constant locals they use).

use crate::hir::ClosureKind;
use crate::thir::{PromotedId, ThirClosure, ThirExprId, ThirPatId, ThirPromoted};
use crate::ty::{Ty, TyKind};

use super::{BlockAnd, Builder, ItemCx};
use crate::mir::{Body, BodyKind, ItemMir, MirSource, Place, Terminator};

impl ItemCx<'_> {
    pub(super) fn build(&self) -> ItemMir {
        let (main, loops) = self.main_body();
        let mut closures: Vec<_> = self
            .thir
            .closures
            .iter()
            .enumerate()
            .map(|(index, closure)| (closure.src, self.closure_body(index, closure)))
            .collect();
        closures.sort_by_key(|(src, _)| *src);
        let mut promoted = Vec::with_capacity(self.thir.promoted.len());
        let mut promoted_src = Vec::with_capacity(self.thir.promoted.len());
        for (index, data) in self.thir.promoted.iter().enumerate() {
            let Some(index) = PromotedId::from_usize(index) else {
                break;
            };
            promoted.push(self.promoted_body(index, data));
            promoted_src.push(data.src);
        }
        ItemMir {
            main,
            closures,
            promoted,
            promoted_src,
            loops,
        }
    }

    fn main_body(&self) -> (Body, Vec<crate::mir::LoopInfo>) {
        let thir = self.thir;
        let sig = self.sigs.fn_sig(self.item);
        let ret = match sig {
            Some(sig) => sig.ret.clone(),
            None => thir.expr(thir.root).ty.clone(),
        };
        let mut builder = Builder::new(self, MirSource::Item(self.item), self.kind(), ret);
        let mut tys: Vec<Ty> = sig.map(|sig| sig.params.clone()).unwrap_or_default();
        if let Some(local) = thir.self_param {
            let ty = if tys.is_empty() {
                Ty::error()
            } else {
                tys.remove(0)
            };
            builder.arg(Some(local), ty);
        }
        let destructure = builder.params(&thir.params, &tys);
        builder.body(destructure, &[], thir.root);
        builder.finish()
    }

    fn closure_body(&self, index: usize, closure: &ThirClosure) -> Body {
        let kind = match closure.kind {
            ClosureKind::Plain => BodyKind::Fn,
            ClosureKind::Const => BodyKind::ConstFn,
            ClosureKind::Pushdown => BodyKind::Instant,
        };
        let source = MirSource::Closure {
            owner: self.item,
            expr: closure.src,
        };
        let mut builder = Builder::new(self, source, kind, closure.ret.clone());
        for &local in &closure.captures {
            let ty = self
                .bindings
                .get(&local)
                .map_or_else(Ty::error, |decl| decl.ty.clone());
            builder.arg(Some(local), ty);
        }
        let tys: Vec<Ty> = u32::try_from(index)
            .ok()
            .and_then(|index| self.closure_tys.get(&index))
            .and_then(|ty| match ty.kind() {
                TyKind::Closure { params, .. } => Some(params.to_vec()),
                _ => None,
            })
            .unwrap_or_default();
        let destructure = builder.params(&closure.params, &tys);
        builder.body(destructure, &[], closure.body);
        builder.finish().0
    }

    fn promoted_body(&self, index: PromotedId, data: &ThirPromoted) -> Body {
        let source = MirSource::Promoted {
            owner: self.item,
            index,
        };
        let ret = self.thir.expr(data.root).ty.clone();
        let mut builder = Builder::new(self, source, BodyKind::Const, ret);
        builder.body(Vec::new(), &data.const_locals, data.root);
        builder.finish().0
    }
}

impl Builder<'_> {
    /// Lowers a body: destructures the arguments, binds the `prelude` (`let pat = init;`),
    /// then writes `root` to the return place and returns.
    fn body(
        &mut self,
        destructure: Vec<(crate::mir::Local, ThirPatId)>,
        prelude: &[(ThirPatId, ThirExprId)],
        root: ThirExprId,
    ) {
        let mut block = Self::entry();
        for (arg, pat) in destructure {
            block = self.bind_irrefutable(block, Place::local(arg), pat);
        }
        for &(pat, init) in prelude {
            unpack!(block = self.let_binding(block, pat, init));
        }
        let BlockAnd(end, ()) = self.expr_into_dest(Some(Place::local(Body::RETURN)), block, root);
        self.with_src(root, |this| this.terminate(end, Terminator::Return));
    }
}
