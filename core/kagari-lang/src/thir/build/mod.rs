//! Building the THIR of one item from its HIR body, name resolution and inference results.

mod call;
mod expr;
mod pat;
mod promote;

use la_arena::Arena;

use crate::db::Db;
use crate::diagnostic::{Diag, Severity};
use crate::eval::ConstExprs;
use crate::eval::const_exprs;
use crate::eval::std_items::{StdTable, std_table};
use crate::hir::{Body, ExprId, body_of};
use crate::infer::{InferenceResult, infer};
use crate::input::Program;
use crate::item_tree::{FnKind, ItemId, ItemKind, item_tree};
use crate::resolve::{BodyResolution, LocalId, body_resolution};
use crate::signatures::{ProgramSignatures, program_signatures};
use crate::ty::{Ty, TyKind};

use super::{
    PromotedId, Thir, ThirClosure, ThirExpr, ThirExprId, ThirExprKind, ThirPat, ThirPatId,
    ThirPromoted,
};

/// The THIR of `item`'s body: `None` when the item has no body, when its resolution or
/// inference reported an error, or when std failed to load.
#[salsa::tracked(returns(ref))]
pub fn thir(db: &dyn Db, program: Program, item: ItemId) -> Option<Thir> {
    let body = body_of(db, item)?;
    let resolution = body_resolution(db, program, item);
    if has_error(&resolution.diagnostics) {
        return None;
    }
    let res = resolution.body.as_ref()?;
    let inferred = infer(db, program, item).as_ref()?;
    if has_error(&inferred.diagnostics) {
        return None;
    }
    let std = std_table(db, program).as_ref()?;
    let sigs = program_signatures(db, program);
    // Promotion covers the bodies whose constant parts const evaluation folds: those of
    // `pushdown fn` items.
    let story = matches!(
        item_tree(db, item.file)
            .item(item.index)
            .map(|data| &data.kind),
        Some(ItemKind::Fn {
            kind: FnKind::Pushdown,
            has_body: true,
            ..
        })
    );
    let promote = if story {
        const_exprs(db, program, item)
    } else {
        None
    };
    let builder = Builder {
        db,
        item,
        body,
        res,
        infer: &inferred.result,
        sigs,
        std,
        promote,
        exprs: Arena::default(),
        pats: Arena::default(),
        closures: Vec::new(),
        promoted: Vec::new(),
    };
    Some(builder.build())
}

fn has_error(diagnostics: &[Diag]) -> bool {
    diagnostics
        .iter()
        .any(|diag| diag.severity() == Severity::Error)
}

pub(super) struct Builder<'a> {
    db: &'a dyn Db,
    item: ItemId,
    body: &'a Body,
    res: &'a BodyResolution,
    infer: &'a InferenceResult,
    sigs: &'a ProgramSignatures,
    std: &'a StdTable,
    /// The constant expressions of a body whose maximal constant subexpressions are
    /// promoted; `None` while building inside a promoted (or trivially constant)
    /// subexpression, and for other bodies.
    promote: Option<&'a ConstExprs>,
    exprs: Arena<ThirExpr>,
    pats: Arena<ThirPat>,
    closures: Vec<ThirClosure>,
    promoted: Vec<ThirPromoted>,
}

impl<'a> Builder<'a> {
    fn build(mut self) -> Thir {
        let body = self.body;
        let self_param: Option<LocalId> = if body.has_self {
            self.res
                .locals
                .iter()
                .find(|(_, local)| local.pat.is_none())
                .map(|(id, _)| id)
        } else {
            None
        };
        // The parameter types follow the `self` parameter's.
        let param_tys: Vec<Ty> = match self.sigs.fn_sig(self.item) {
            Some(sig) => sig
                .params
                .iter()
                .skip(usize::from(body.has_self))
                .cloned()
                .collect(),
            None => Vec::new(),
        };
        let params = body
            .params
            .iter()
            .enumerate()
            .map(|(index, &pat)| self.root_pat(pat, param_tys.get(index)))
            .collect();
        let root = self.expr(body.value);
        Thir {
            exprs: self.exprs,
            pats: self.pats,
            self_param,
            params,
            root,
            closures: self.closures,
            promoted: self.promoted,
        }
    }

    fn alloc(&mut self, kind: ThirExprKind, ty: Ty, src: ExprId) -> ThirExprId {
        self.exprs.alloc(ThirExpr { kind, ty, src })
    }

    fn error(&mut self, ty: Ty, src: ExprId) -> ThirExprId {
        self.alloc(ThirExprKind::Error, ty, src)
    }

    /// The type inference gave `expr`, before its coercion.
    fn expr_ty(&self, expr: ExprId) -> Ty {
        self.infer
            .expr_types
            .get(expr)
            .cloned()
            .unwrap_or_else(Ty::error)
    }

    fn ty_of(&self, id: ThirExprId) -> Ty {
        self.exprs[id].ty.clone()
    }

    /// `node` dereferenced `count` times; the synthesized nodes carry `src`.
    fn deref_n(&mut self, mut node: ThirExprId, count: u32, src: ExprId) -> ThirExprId {
        for _ in 0..count {
            let ty = match self.exprs[node].ty.kind() {
                TyKind::Ref { inner, .. } => inner.clone(),
                _ => Ty::error(),
            };
            node = self.alloc(ThirExprKind::Deref(node), ty, src);
        }
        node
    }

    fn next_promoted(&self) -> Option<PromotedId> {
        PromotedId::from_usize(self.promoted.len())
    }

    fn next_closure(&self) -> Option<u32> {
        u32::try_from(self.closures.len()).ok()
    }

    fn pat_id(&mut self, pat: ThirPat) -> ThirPatId {
        self.pats.alloc(pat)
    }
}

/// The number of `TyKind::Ref` layers of `ty`.
fn ref_depth(ty: &Ty) -> u32 {
    let mut depth = 0u32;
    let mut current = ty;
    while let TyKind::Ref { inner, .. } = current.kind() {
        depth = depth.saturating_add(1);
        current = inner;
    }
    depth
}
