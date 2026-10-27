//! Expressions and statements.

use crate::hir::{ArrayExpr, BinaryOp, Condition, Expr, ExprId, Name, Stmt, UnaryOp};
use crate::infer::ValueRef;
use crate::item_tree::ItemKind;
use crate::item_tree::item_tree;
use crate::signatures::ItemSig;
use crate::thir::{
    ForKind, ThirArm, ThirClosure, ThirCond, ThirExprId, ThirExprKind, ThirStmt, TryKind,
    literal_value,
};
use crate::ty::{Const, CtorRef, FnDefId, FnRef, GenericArg, GenericArgs, Ty, TyKind};

use super::{Builder, ref_depth};

impl Builder<'_> {
    /// The THIR of `expr`, wrapped in its coercion.
    pub(super) fn expr(&mut self, expr: ExprId) -> ThirExprId {
        stacker::maybe_grow(crate::STACK_RED_ZONE, crate::STACK_GROWTH, || {
            let node = self.expr_uncoerced(expr);
            match self.infer.coercions.get(&expr) {
                Some((kind, ty)) => self.alloc(
                    ThirExprKind::Coerce {
                        kind: *kind,
                        expr: node,
                    },
                    ty.clone(),
                    expr,
                ),
                None => node,
            }
        })
    }

    fn expr_uncoerced(&mut self, expr: ExprId) -> ThirExprId {
        let ty = self.expr_ty(expr);
        if ty.references_error() {
            return self.error(ty, expr);
        }
        if let Some(consts) = self.promote
            && consts.contains(expr)
        {
            return self.promote_const(expr, ty, consts);
        }
        self.lower(expr, ty)
    }

    /// The THIR of `expr` itself, of type `ty`.
    pub(super) fn lower(&mut self, expr: ExprId, ty: Ty) -> ThirExprId {
        let body = self.body;
        let kind = match body.store.expr(expr) {
            Expr::Missing => ThirExprKind::Error,
            Expr::Literal(literal) => match literal_value(literal, false) {
                Some(value) => ThirExprKind::Literal(value),
                None => ThirExprKind::Error,
            },
            Expr::Path(_) => self.path(expr),
            Expr::Block { stmts, tail, .. } => {
                let stmts = self.stmts(stmts);
                let tail = tail.map(|tail| self.expr(tail));
                ThirExprKind::Block { stmts, tail }
            }
            Expr::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let cond = self.cond(cond);
                let then = self.expr(*then_branch);
                let else_ = else_branch.map(|branch| self.expr(branch));
                ThirExprKind::If { cond, then, else_ }
            }
            Expr::Match { scrutinee, arms } => {
                let scrutinee = self.expr(*scrutinee);
                let scrutinee_ty = self.ty_of(scrutinee);
                let arms = arms
                    .iter()
                    .map(|arm| ThirArm {
                        pat: self.root_pat(arm.pat, Some(&scrutinee_ty)),
                        guard: arm.guard.map(|guard| self.expr(guard)),
                        body: self.expr(arm.body),
                    })
                    .collect();
                ThirExprKind::Match { scrutinee, arms }
            }
            Expr::Loop { body, .. } => ThirExprKind::Loop {
                body: self.expr(*body),
            },
            Expr::While { cond, body, .. } => {
                let cond = self.cond(cond);
                let body = self.expr(*body);
                ThirExprKind::While { cond, body }
            }
            Expr::For {
                pat,
                iterable,
                body,
                ..
            } => self.for_loop(*pat, *iterable, *body),
            Expr::Break { value, .. } => match self.res.loop_targets.get(&expr).copied() {
                Some(target) => ThirExprKind::Break {
                    target,
                    value: value.map(|value| self.expr(value)),
                },
                None => ThirExprKind::Error,
            },
            Expr::Continue { .. } => match self.res.loop_targets.get(&expr).copied() {
                Some(target) => ThirExprKind::Continue { target },
                None => ThirExprKind::Error,
            },
            Expr::Return { value } => ThirExprKind::Return {
                value: value.map(|value| self.expr(value)),
            },
            Expr::Call { callee, args } => self.call(*callee, args),
            Expr::MethodCall { receiver, args, .. } => self.method_call(expr, *receiver, args),
            Expr::Field { base, .. } => match self.infer.fields.get(&expr).copied() {
                Some(field) => {
                    let base = self.expr(*base);
                    let base = self.deref_n(base, field.derefs, expr);
                    ThirExprKind::Field {
                        base,
                        index: field.index,
                    }
                }
                None => ThirExprKind::Error,
            },
            Expr::Index { base, index } => {
                // Indexing follows references to the array or `Vec`.
                let base = self.expr(*base);
                let depth = ref_depth(&self.exprs[base].ty);
                let base = self.deref_n(base, depth, expr);
                let index = self.expr(*index);
                ThirExprKind::Index { base, index }
            }
            Expr::Try(inner) => {
                let inner = self.expr(*inner);
                match self.try_kind(&self.ty_of(inner)) {
                    Some(kind) => ThirExprKind::Try { expr: inner, kind },
                    None => ThirExprKind::Error,
                }
            }
            Expr::Cast { expr: inner, .. } => ThirExprKind::Cast {
                expr: self.expr(*inner),
                ty: ty.clone(),
            },
            Expr::Unary {
                op: UnaryOp::Deref,
                expr: inner,
            } => ThirExprKind::Deref(self.expr(*inner)),
            Expr::Unary { op, expr: inner } => ThirExprKind::Unary {
                op: *op,
                expr: self.expr(*inner),
            },
            Expr::Ref {
                mutable,
                expr: inner,
            } => ThirExprKind::Borrow {
                mutable: *mutable,
                place: self.expr(*inner),
            },
            Expr::Binary { op, lhs, rhs } => {
                let (lhs, rhs) = (self.expr(*lhs), self.expr(*rhs));
                match op {
                    BinaryOp::And | BinaryOp::Or => ThirExprKind::Logical {
                        and: *op == BinaryOp::And,
                        lhs,
                        rhs,
                    },
                    _ => ThirExprKind::Binary { op: *op, lhs, rhs },
                }
            }
            Expr::Assign { op, target, value } => {
                // Compound assignment to an `Int<lo, hi>` place saturates at its bounds.
                let saturate = matches!(self.expr_ty(*target).kind(), TyKind::Bounded { .. });
                let value = self.expr(*value);
                let target = self.expr(*target);
                match op {
                    Some(op) => ThirExprKind::AssignOp {
                        op: *op,
                        target,
                        value,
                        saturate,
                    },
                    None => ThirExprKind::Assign { target, value },
                }
            }
            Expr::Range {
                start,
                end,
                inclusive,
            } => self.range(expr, &ty, *start, *end, *inclusive),
            Expr::Tuple(items) => ThirExprKind::Tuple(self.list(items)),
            Expr::Array(ArrayExpr::List(items)) => ThirExprKind::Array(self.list(items)),
            Expr::Array(ArrayExpr::Repeat { value, .. }) => {
                // The count is a constant: the array type's length.
                let count = match ty.kind() {
                    TyKind::Array {
                        len: Const::Value(len),
                        ..
                    } => u64::try_from(*len).ok(),
                    _ => None,
                };
                match count {
                    Some(count) => ThirExprKind::Repeat {
                        value: self.expr(*value),
                        count,
                    },
                    None => ThirExprKind::Error,
                }
            }
            Expr::Record { fields, .. } => self.record(expr, fields),
            Expr::Closure { params, body, .. } => self.closure(expr, &ty, params, *body),
        };
        self.alloc(kind, ty, expr)
    }

    fn list(&mut self, items: &[ExprId]) -> Vec<ThirExprId> {
        items.iter().map(|item| self.expr(*item)).collect()
    }

    fn stmts(&mut self, stmts: &[Stmt]) -> Vec<ThirStmt> {
        let mut out = Vec::with_capacity(stmts.len());
        for stmt in stmts {
            match stmt {
                Stmt::Let { pat, init, .. } => {
                    let init = init.map(|init| self.expr(init));
                    let expected = init.map(|init| self.ty_of(init));
                    let pat = self.root_pat(*pat, expected.as_ref());
                    out.push(ThirStmt::Let { pat, init });
                }
                Stmt::Expr { expr, .. } => out.push(ThirStmt::Expr(self.expr(*expr))),
                Stmt::Item(_) => {}
            }
        }
        out
    }

    fn cond(&mut self, cond: &Condition) -> ThirCond {
        match cond {
            Condition::Expr(expr) => ThirCond::Expr(self.expr(*expr)),
            Condition::Let { pat, expr } => {
                let expr = self.expr(*expr);
                let expected = self.ty_of(expr);
                ThirCond::Let {
                    pat: self.root_pat(*pat, Some(&expected)),
                    expr,
                }
            }
        }
    }

    fn path(&mut self, expr: ExprId) -> ThirExprKind {
        let Some(path) = self.infer.paths.get(&expr) else {
            return ThirExprKind::Error;
        };
        match path {
            ValueRef::Local(local) => ThirExprKind::Local(*local),
            ValueRef::Fn(fn_ref) => ThirExprKind::FnItem(fn_ref.clone()),
            ValueRef::Ctor(ctor) => {
                let has_fields = self
                    .sigs
                    .adt(ctor.adt)
                    .and_then(|adt| adt.variants.get(ctor.variant.unwrap_or(0) as usize))
                    .is_some_and(|variant| !variant.fields.is_empty());
                if has_fields {
                    let def = match ctor.variant {
                        Some(variant) => FnDefId::VariantCtor(ctor.adt, variant),
                        None => FnDefId::StructCtor(ctor.adt),
                    };
                    ThirExprKind::FnItem(FnRef {
                        def,
                        self_ty: None,
                        args: ctor.args.clone(),
                    })
                } else {
                    ThirExprKind::Adt {
                        ctor: ctor.clone(),
                        fields: Vec::new(),
                    }
                }
            }
            ValueRef::Const { item, args } => ThirExprKind::ConstItem {
                item: *item,
                args: GenericArgs::new(args.clone()),
            },
            ValueRef::Static(item) => {
                let story_var = matches!(
                    item_tree(self.db, item.file)
                        .item(item.index)
                        .map(|data| &data.kind),
                    Some(ItemKind::Static { mutable: true })
                );
                if story_var {
                    return ThirExprKind::StoryVar(*item);
                }
                // A `static` of type `Setting<T>`/`Meta<T>` is a handle.
                let handle = matches!(
                    self.sigs.get(*item),
                    Some(ItemSig::Static { ty, .. })
                        if matches!(ty.kind(), TyKind::Adt { item: adt, .. }
                            if *adt == self.std.setting || *adt == self.std.meta)
                );
                if handle {
                    ThirExprKind::Handle(*item)
                } else {
                    ThirExprKind::StaticValue(*item)
                }
            }
            ValueRef::ConstParam { owner, index } => ThirExprKind::ConstParam {
                owner: *owner,
                index: *index,
            },
            ValueRef::Error => ThirExprKind::Error,
        }
    }

    fn for_loop(&mut self, pat: crate::hir::PatId, iterable: ExprId, body: ExprId) -> ThirExprKind {
        let iterable_hir = self.body.store.expr(iterable);
        let literal_inclusive = match iterable_hir {
            Expr::Range { inclusive, .. } => Some(*inclusive),
            _ => None,
        };
        let iterable = self.expr(iterable);
        let iterable_ty = self.ty_of(iterable);
        let first_arg = |args: &GenericArgs| match args.first() {
            Some(GenericArg::Type(elem)) => Some(elem.clone()),
            _ => None,
        };
        let (kind, elem) = match iterable_ty.kind() {
            TyKind::Array { elem, .. } => (ForKind::Array, Some(elem.clone())),
            TyKind::Adt { item, args } if Some(*item) == self.sigs.std.vec => {
                (ForKind::Vec, first_arg(args))
            }
            TyKind::Adt { item, args } if *item == self.std.range.item => (
                ForKind::Range {
                    inclusive: literal_inclusive,
                },
                first_arg(args),
            ),
            _ => return ThirExprKind::Error,
        };
        let pat = self.root_pat(pat, elem.as_ref());
        let body = self.expr(body);
        ThirExprKind::For {
            pat,
            iterable,
            body,
            kind,
        }
    }

    fn try_kind(&self, ty: &Ty) -> Option<TryKind> {
        let TyKind::Adt { item, .. } = ty.kind() else {
            return None;
        };
        let (option, result) = (&self.std.option, &self.std.result);
        if *item == option.item {
            Some(TryKind::Option {
                some: option.variant("Some")?,
                none: option.variant("None")?,
            })
        } else if *item == result.item {
            Some(TryKind::Result {
                ok: result.variant("Ok")?,
                err: result.variant("Err")?,
            })
        } else {
            None
        }
    }

    /// `start..end` and `start..=end`: a `Range` value.
    fn range(
        &mut self,
        expr: ExprId,
        ty: &Ty,
        start: Option<ExprId>,
        end: Option<ExprId>,
        inclusive: bool,
    ) -> ThirExprKind {
        let range = &self.std.range;
        let field = |name: &str| {
            range
                .field(name)
                .and_then(|index| u32::try_from(index).ok())
        };
        let (Some(start), Some(end)) = (start, end) else {
            return ThirExprKind::Error;
        };
        let (Some(start_index), Some(end_index), Some(inclusive_index)) =
            (field("start"), field("end"), field("inclusive"))
        else {
            return ThirExprKind::Error;
        };
        let TyKind::Adt { item, args } = ty.kind() else {
            return ThirExprKind::Error;
        };
        let ctor = CtorRef {
            adt: *item,
            variant: None,
            args: args.clone(),
        };
        let start = self.expr(start);
        let end = self.expr(end);
        let flag = self.alloc(
            ThirExprKind::Literal(crate::eval::ConstValue::Bool(inclusive)),
            Ty::bool(),
            expr,
        );
        ThirExprKind::Adt {
            ctor,
            fields: vec![
                (start_index, start),
                (end_index, end),
                (inclusive_index, flag),
            ],
        }
    }

    fn record(&mut self, expr: ExprId, fields: &[(Name, ExprId)]) -> ThirExprKind {
        let Some(ctor) = self.infer.records.get(&expr).cloned() else {
            return ThirExprKind::Error;
        };
        let Some(variant) = self
            .sigs
            .adt(ctor.adt)
            .and_then(|adt| adt.variants.get(ctor.variant.unwrap_or(0) as usize))
        else {
            return ThirExprKind::Error;
        };
        let mut out = Vec::with_capacity(fields.len());
        for (name, value) in fields {
            let Some(index) = variant
                .fields
                .iter()
                .position(|field| field.name.as_ref() == Some(name))
                .and_then(|index| u32::try_from(index).ok())
            else {
                return ThirExprKind::Error;
            };
            out.push((index, self.expr(*value)));
        }
        ThirExprKind::Adt { ctor, fields: out }
    }

    fn closure(
        &mut self,
        expr: ExprId,
        ty: &Ty,
        params: &[(crate::hir::PatId, Option<crate::hir::TypeRefId>)],
        body: ExprId,
    ) -> ThirExprKind {
        let Some(info) = self.infer.closures.get(&expr) else {
            return ThirExprKind::Error;
        };
        let TyKind::Closure {
            params: param_tys,
            ret,
            ..
        } = ty.kind()
        else {
            return ThirExprKind::Error;
        };
        let params = params
            .iter()
            .enumerate()
            .map(|(index, (pat, _))| self.root_pat(*pat, param_tys.get(index)))
            .collect();
        let body = self.expr(body);
        let Some(index) = self.next_closure() else {
            return ThirExprKind::Error;
        };
        self.closures.push(ThirClosure {
            src: expr,
            kind: info.kind,
            captures: info.captures.clone(),
            params,
            body,
            ret: ret.clone(),
        });
        ThirExprKind::Closure { index }
    }
}
