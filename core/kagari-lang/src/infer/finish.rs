//! The end of a body's inference: literal defaults, deferred checks, and the resolution of
//! every recorded type.

use rustc_hash::FxHashSet;

use super::ctx::{InferCtx, ObligationKind};
use super::{Coercion, InferenceResult, MethodCall, ValueRef};
use crate::diagnostic::DiagnosticCode;
use crate::hir::ExprId;
use crate::signatures::lower::item_name;
use crate::signatures::{TraitSolver, display_ty};
use crate::ty::{Const, CtorRef, FnRef, GenericArg, InferVar, Ty, TyNode};

impl InferCtx<'_> {
    pub(crate) fn finish(mut self) -> (InferenceResult, Vec<crate::diagnostic::Diagnostic>) {
        self.process_projections();
        self.resolve_links();
        self.default_literal_bounds();
        for (var, literal) in self.table.unresolved_literals() {
            self.table
                .unify(&Ty::Infer(var), &Ty::bounded(literal.lo, literal.hi));
        }
        self.process_projections();
        self.check_fits();
        self.check_pat_fits();
        self.check_obligations();
        self.check_intos();
        self.resolve_result();
        (self.result, self.diags)
    }

    /// A literal bound to `Int<?lo, ?hi>` fixes the unknown bounds to its own.
    fn default_literal_bounds(&mut self) {
        let fits = self.table.fits.clone();
        for fit in fits {
            if let Ty::Bounded { lo, hi } = self.table.shallow(&fit.ty) {
                if matches!(self.table.shallow_const(&lo), Const::Infer(_)) {
                    self.table.unify_const(&lo, &Const::Value(fit.literal.lo));
                }
                if matches!(self.table.shallow_const(&hi), Const::Infer(_)) {
                    self.table.unify_const(&hi, &Const::Value(fit.literal.hi));
                }
            }
        }
    }

    fn check_fits(&mut self) {
        let fits = self.table.fits.clone();
        let mut reported: FxHashSet<ExprId> = FxHashSet::default();
        for fit in fits {
            let Some((lo, hi)) = self.int_range(&fit.ty) else {
                continue;
            };
            if fit.literal.lo >= lo && fit.literal.hi <= hi {
                continue;
            }
            if !reported.insert(fit.literal.origin) {
                continue;
            }
            let range = self.expr_range(fit.literal.origin);
            let value = if fit.literal.lo < lo {
                fit.literal.lo
            } else {
                fit.literal.hi
            };
            let message = format!(
                "literal out of range for `{}`: `{value}` does not fit",
                self.show(&fit.ty)
            );
            self.diag(range, DiagnosticCode::TypeLiteralOutOfRange, message);
        }
    }

    /// Integer literals of patterns must fit in the matched type.
    fn check_pat_fits(&mut self) {
        let fits = std::mem::take(&mut self.pat_fits);
        for fit in fits {
            let resolved = self.table.resolve(&fit.ty);
            if let Some((lo, hi)) = self.int_range(&resolved) {
                if fit.value < lo || fit.value > hi {
                    let message = format!(
                        "literal out of range for `{}`: `{}` does not fit",
                        self.show(&resolved),
                        fit.value
                    );
                    self.diag(fit.range, DiagnosticCode::TypeLiteralOutOfRange, message);
                }
            } else if !matches!(resolved, Ty::Infer(_) | Ty::Bounded { .. })
                && !resolved.references_error()
            {
                let message = format!(
                    "mismatched types: expected `{}`, found an integer",
                    self.show(&resolved)
                );
                self.diag(fit.range, DiagnosticCode::TypeMismatch, message);
            }
        }
    }

    fn check_obligations(&mut self) {
        let obligations = std::mem::take(&mut self.obligations);
        let mut solver = TraitSolver {
            sigs: self.sigs,
            maps: self.maps,
            env: &self.env,
            visiting: FxHashSet::default(),
        };
        let mut failures = Vec::new();
        for obligation in &obligations {
            let ty = self.table.resolve(&obligation.ty);
            if ty.has_infer() || ty.references_error() {
                continue;
            }
            if !solver.implements(&ty, obligation.trait_item) {
                failures.push((ty, obligation.trait_item, obligation.range, obligation.kind));
            }
        }
        for (ty, trait_item, range, kind) in failures {
            let shown = display_ty(self.db, self.program, &ty);
            match kind {
                ObligationKind::Eq => {
                    let message = if matches!(ty, Ty::Closure { .. }) {
                        format!("`==` needs `Eq`, but closures such as `{shown}` are never `Eq`")
                    } else {
                        format!("`==` needs `Eq`, but `{shown}` does not implement it")
                    };
                    self.diag(range, DiagnosticCode::TypeNotEq, message);
                }
                ObligationKind::Bound => {
                    let name = item_name(self.db, trait_item);
                    self.diag(
                        range,
                        DiagnosticCode::TypeUnsatisfiedBound,
                        format!("the trait bound `{shown}: {name}` is not satisfied"),
                    );
                }
            }
        }
    }

    fn check_intos(&mut self) {
        let checks = std::mem::take(&mut self.into_checks);
        for check in checks {
            let from = self.table.resolve(&check.from);
            let to = self.table.resolve(&check.to);
            let (Some((l1, h1)), Some((l2, h2))) = (self.int_range(&from), self.int_range(&to))
            else {
                continue;
            };
            if l2 <= l1 && h1 <= h2 {
                continue;
            }
            let message = format!(
                "`{}` does not always fit in `{}`; use `.saturate()` to clamp it",
                self.show(&from),
                self.show(&to)
            );
            self.diag(check.range, DiagnosticCode::TypeLossyInto, message);
        }
    }

    fn resolve_result(&mut self) {
        let report = !self.has_errors();
        let mut reported: FxHashSet<InferVar> = FxHashSet::default();
        let mut result = std::mem::take(&mut self.result);
        let expr_types: Vec<(ExprId, Ty)> = result
            .expr_types
            .iter()
            .map(|(expr, ty)| (expr, self.table.resolve(ty)))
            .collect();
        if report {
            // Report each unresolved variable once, preferring a value's type over the type
            // of the function it comes from (`Vec<_>` rather than `fn() -> Vec<_> {new}`).
            for fn_items in [false, true] {
                for (expr, resolved) in &expr_types {
                    if matches!(resolved, Ty::FnDef(_)) != fn_items || resolved.references_error() {
                        continue;
                    }
                    for var in unresolved_roots(resolved) {
                        if reported.insert(var) {
                            let range = self.expr_range(*expr);
                            let message = format!(
                                "type annotations needed: cannot infer the type `{}`",
                                self.show(resolved)
                            );
                            self.diag(range, DiagnosticCode::TypeAnnotationsNeeded, message);
                        }
                    }
                }
            }
        }
        for (expr, resolved) in expr_types {
            result.expr_types.insert(expr, self.finalize_ty(&resolved));
        }
        for (expr, _) in self.store.exprs.iter() {
            if result.expr_types.get(expr).is_none() {
                result.expr_types.insert(expr, Ty::Error);
            }
        }
        let pats: Vec<_> = result
            .pat_types
            .iter()
            .map(|(pat, ty)| (pat, self.finalize_ty(ty)))
            .collect();
        for (pat, ty) in pats {
            result.pat_types.insert(pat, ty);
        }
        for (pat, _) in self.store.pats.iter() {
            if result.pat_types.get(pat).is_none() {
                result.pat_types.insert(pat, Ty::Error);
            }
        }
        let locals: Vec<_> = result
            .local_types
            .iter()
            .map(|(local, ty)| (local, self.finalize_ty(ty)))
            .collect();
        for (local, ty) in locals {
            result.local_types.insert(local, ty);
        }
        for (local, _) in self.res.locals.iter() {
            if result.local_types.get(local).is_none() {
                result.local_types.insert(local, Ty::Error);
            }
        }
        result.return_ty = self.finalize_ty(&result.return_ty.clone());
        result.paths = std::mem::take(&mut result.paths)
            .into_iter()
            .map(|(expr, value)| (expr, self.finalize_value(value)))
            .collect();
        result.method_calls = std::mem::take(&mut result.method_calls)
            .into_iter()
            .map(|(expr, call)| {
                (
                    expr,
                    MethodCall {
                        func: self.finalize_fn_ref(&call.func),
                        receiver: call.receiver,
                    },
                )
            })
            .collect();
        result.records = std::mem::take(&mut result.records)
            .into_iter()
            .map(|(expr, ctor)| (expr, self.finalize_ctor(&ctor)))
            .collect();
        result.pat_ctors = std::mem::take(&mut result.pat_ctors)
            .into_iter()
            .map(|(pat, ctor)| (pat, self.finalize_ctor(&ctor)))
            .collect();
        result.coercions = std::mem::take(&mut result.coercions)
            .into_iter()
            .map(|(expr, (coercion, ty)): (ExprId, (Coercion, Ty))| {
                (expr, (coercion, self.finalize_ty(&ty)))
            })
            .collect();
        self.result = result;
    }

    /// `ty` fully resolved, with unresolved variables replaced by errors.
    fn finalize_ty(&self, ty: &Ty) -> Ty {
        let resolved = self.table.resolve(ty);
        if !resolved.has_infer() {
            return resolved;
        }
        erase_infer(&resolved)
    }

    fn finalize_args(&self, args: &[GenericArg]) -> Vec<GenericArg> {
        args.iter()
            .map(|arg| match arg {
                GenericArg::Type(ty) => GenericArg::Type(self.finalize_ty(ty)),
                GenericArg::Const(c) => GenericArg::Const(match self.table.shallow_const(c) {
                    Const::Infer(_) => Const::Error,
                    other => other,
                }),
            })
            .collect()
    }

    fn finalize_fn_ref(&self, fn_ref: &FnRef) -> FnRef {
        FnRef {
            def: fn_ref.def,
            self_ty: fn_ref.self_ty.as_ref().map(|ty| self.finalize_ty(ty)),
            args: self.finalize_args(&fn_ref.args),
        }
    }

    fn finalize_ctor(&self, ctor: &CtorRef) -> CtorRef {
        CtorRef {
            adt: ctor.adt,
            variant: ctor.variant,
            args: self.finalize_args(&ctor.args),
        }
    }

    fn finalize_value(&self, value: ValueRef) -> ValueRef {
        match value {
            ValueRef::Fn(fn_ref) => ValueRef::Fn(self.finalize_fn_ref(&fn_ref)),
            ValueRef::Ctor(ctor) => ValueRef::Ctor(self.finalize_ctor(&ctor)),
            ValueRef::Const { item, args } => ValueRef::Const {
                item,
                args: self.finalize_args(&args),
            },
            other => other,
        }
    }
}

fn unresolved_roots(ty: &Ty) -> Vec<InferVar> {
    let mut vars = Vec::new();
    ty.walk(&mut |node| {
        if let TyNode::Ty(Ty::Infer(var)) | TyNode::Const(Const::Infer(var)) = node
            && !vars.contains(var)
        {
            vars.push(*var);
        }
    });
    vars
}

fn erase_infer(ty: &Ty) -> Ty {
    let erase_const = |c: &Const| match c {
        Const::Infer(_) => Const::Error,
        other => other.clone(),
    };
    let erase_args = |args: &[GenericArg]| -> Vec<GenericArg> {
        args.iter()
            .map(|arg| match arg {
                GenericArg::Type(ty) => GenericArg::Type(erase_infer(ty)),
                GenericArg::Const(c) => GenericArg::Const(erase_const(c)),
            })
            .collect()
    };
    match ty {
        Ty::Infer(_) => Ty::Error,
        Ty::Bounded { lo, hi } => Ty::Bounded {
            lo: erase_const(lo),
            hi: erase_const(hi),
        },
        Ty::Tuple(fields) => Ty::Tuple(fields.iter().map(erase_infer).collect()),
        Ty::Array { elem, len } => Ty::Array {
            elem: Box::new(erase_infer(elem)),
            len: erase_const(len),
        },
        Ty::Adt { item, args } => Ty::Adt {
            item: *item,
            args: erase_args(args),
        },
        Ty::Ref { mutable, inner } => Ty::Ref {
            mutable: *mutable,
            inner: Box::new(erase_infer(inner)),
        },
        Ty::FnPtr { kind, params, ret } => Ty::FnPtr {
            kind: *kind,
            params: params.iter().map(erase_infer).collect(),
            ret: Box::new(erase_infer(ret)),
        },
        Ty::Closure { kind, params, ret } => Ty::Closure {
            kind: *kind,
            params: params.iter().map(erase_infer).collect(),
            ret: Box::new(erase_infer(ret)),
        },
        Ty::FnDef(fn_ref) => Ty::FnDef(Box::new(FnRef {
            def: fn_ref.def,
            self_ty: fn_ref.self_ty.as_ref().map(erase_infer),
            args: erase_args(&fn_ref.args),
        })),
        Ty::Projection {
            base,
            trait_item,
            name,
        } => Ty::Projection {
            base: Box::new(erase_infer(base)),
            trait_item: *trait_item,
            name: name.clone(),
        },
        other => other.clone(),
    }
}
