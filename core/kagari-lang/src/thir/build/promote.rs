//! Promotion: the maximal constant subexpressions of a `pushdown fn` body become
//! [`ThirPromoted`] constants, computed by const evaluation instead of at runtime. These
//! are exactly the expressions const evaluation has always folded: a constant expression
//! (`eval::const_exprs`) none of whose ancestors is constant, other than a literal, a path,
//! or an expression of type `()` or `!` (whose constant subexpressions are not promoted
//! either). Closure bodies inside the body are searched too.

use rustc_hash::FxHashSet;

use crate::eval::ConstExprs;
use crate::eval::const_exprs::children;
use crate::hir::{Condition, Expr, ExprId, Pat, PatId, Stmt};
use crate::infer::ValueRef;
use crate::resolve::LocalId;
use crate::thir::{ThirExprId, ThirExprKind, ThirPatId, ThirPromoted};
use crate::ty::Ty;

use super::Builder;

impl<'a> Builder<'a> {
    /// The constant `expr` of type `ty`: promoted, or built as is when it is trivial.
    pub(super) fn promote_const(
        &mut self,
        expr: ExprId,
        ty: Ty,
        consts: &'a ConstExprs,
    ) -> ThirExprId {
        let trivial = matches!(self.body.store.expr(expr), Expr::Literal(_) | Expr::Path(_))
            || ty.is_unit()
            || ty.is_never();
        // Nothing inside a constant expression is promoted on its own.
        self.promote = None;
        let root = self.lower(expr, ty.clone());
        let promoted = match (trivial, self.next_promoted()) {
            (false, Some(id)) => {
                let mut done = FxHashSet::default();
                let mut const_locals = Vec::new();
                self.const_locals(expr, consts, &mut done, &mut const_locals);
                self.promoted.push(ThirPromoted {
                    src: expr,
                    root,
                    const_locals,
                });
                Some(id)
            }
            _ => None,
        };
        self.promote = Some(consts);
        match promoted {
            Some(id) => self.alloc(ThirExprKind::Promoted(id), ty, expr),
            None => root,
        }
    }

    /// Appends to `out` a `let pat = source;` for every constant local `expr` uses but does
    /// not bind, each after the ones its source needs.
    fn const_locals(
        &mut self,
        expr: ExprId,
        consts: &ConstExprs,
        done: &mut FxHashSet<(PatId, ExprId)>,
        out: &mut Vec<(ThirPatId, ThirExprId)>,
    ) {
        stacker::maybe_grow(crate::STACK_RED_ZONE, crate::STACK_GROWTH, || {
            for local in self.free_locals(expr) {
                let Some(source) = consts.locals.get(&local).copied() else {
                    continue;
                };
                if !done.insert((source.pat, source.source)) {
                    continue;
                }
                self.const_locals(source.source, consts, done, out);
                let init = self.expr(source.source);
                let expected = self.ty_of(init);
                let pat = self.root_pat(source.pat, Some(&expected));
                out.push((pat, init));
            }
        })
    }

    /// The locals `expr` uses but does not bind, in order of first use.
    fn free_locals(&self, expr: ExprId) -> Vec<LocalId> {
        let store = &self.body.store;
        let mut bound = FxHashSet::default();
        let mut used = Vec::new();
        let mut seen = FxHashSet::default();
        let mut stack = vec![expr];
        while let Some(expr) = stack.pop() {
            let data = store.expr(expr);
            match data {
                Expr::Path(_) => {
                    if let Some(ValueRef::Local(local)) = self.infer.paths.get(&expr)
                        && seen.insert(*local)
                    {
                        used.push(*local);
                    }
                }
                Expr::Block { stmts, .. } => {
                    for stmt in stmts {
                        if let Stmt::Let { pat, .. } = stmt {
                            self.bindings(*pat, &mut bound);
                        }
                    }
                }
                Expr::If {
                    cond: Condition::Let { pat, .. },
                    ..
                }
                | Expr::While {
                    cond: Condition::Let { pat, .. },
                    ..
                }
                | Expr::For { pat, .. } => self.bindings(*pat, &mut bound),
                Expr::Match { arms, .. } => {
                    for arm in arms {
                        self.bindings(arm.pat, &mut bound);
                    }
                }
                Expr::Closure { params, .. } => {
                    for (pat, _) in params {
                        self.bindings(*pat, &mut bound);
                    }
                }
                _ => {}
            }
            // Visit the children in evaluation order.
            stack.extend(children(data).into_iter().rev());
        }
        used.retain(|local| !bound.contains(local));
        used
    }

    /// Adds the locals `pat` binds to `out`.
    fn bindings(&self, pat: PatId, out: &mut FxHashSet<LocalId>) {
        let store = &self.body.store;
        let mut stack = vec![pat];
        while let Some(pat) = stack.pop() {
            match store.pat(pat) {
                Pat::Bind { .. } => {
                    if let Some(local) = self.res.bindings.get(&pat) {
                        out.insert(*local);
                    }
                }
                Pat::Tuple(pats) | Pat::TupleStruct { pats, .. } | Pat::Or(pats) => {
                    stack.extend(pats.iter().copied());
                }
                Pat::Record { fields, .. } => stack.extend(fields.iter().map(|(_, pat)| *pat)),
                Pat::Missing
                | Pat::Wild
                | Pat::Path(_)
                | Pat::Lit { .. }
                | Pat::Range { .. }
                | Pat::Rest => {}
            }
        }
    }
}
