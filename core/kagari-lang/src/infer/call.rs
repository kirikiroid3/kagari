//! Calls, method calls, field accesses and indexing.

use super::ctx::{Expect, FnShape, InferCtx, IntoCheck};
use super::{FieldRef, MethodCall, ReceiverAdjust, ValueRef};
use crate::def_map::{Def, Resolution};
use crate::diagnostic::DiagnosticCode;
use crate::hir::{
    ClosureKind, Expr, ExprId, FieldName, FnPtrKind, GenericArgs, Literal, Name, SelfParamKind,
};
use crate::item_tree::{ItemId, ItemKind, item_tree};
use crate::signatures::lower::{plural, trait_assoc, were};
use crate::signatures::subst::{Subst, subst_ty};
use crate::signatures::{ItemSig, assoc_parent, type_key};
use crate::ty::{FnDefId, FnRef, GenericArg, Ty, TyKind};

enum MethodCandidate {
    Inherent { item: ItemId, impl_item: ItemId },
    Trait { item: ItemId, trait_item: ItemId },
}

impl InferCtx<'_> {
    pub(crate) fn infer_call(
        &mut self,
        expr: ExprId,
        callee: ExprId,
        args: &[ExprId],
        expect: Expect<'_>,
    ) -> Ty {
        let callee_item = match (self.store.expr(callee), self.res.store.exprs.get(&callee)) {
            (Expr::Path(_), Some(Resolution::Def(Def::Item(item)))) => Some(*item),
            _ => None,
        };
        let std = &self.sigs.std;
        if callee_item.is_some() && callee_item == std.nav_start {
            return self.infer_nav_start(expr, callee, args);
        }
        if callee_item.is_some() && callee_item == std.asset_fn {
            self.check_asset_path(args);
        }
        let callee_ty = self.infer_expr(callee, Expect::None);
        let callee_ty = self.structurally_resolve(&callee_ty);
        let range = self.expr_anchor(expr);
        let (params, ret, fn_ref) = match callee_ty.kind() {
            TyKind::FnDef(fn_ref) => match self.fn_shape(fn_ref) {
                Some(FnShape { params, ret, .. }) => (params, ret, Some(fn_ref.clone())),
                None => return self.infer_args_unchecked(args),
            },
            TyKind::FnPtr { params, ret, .. } | TyKind::Closure { params, ret, .. } => {
                (params.to_vec(), ret.clone(), None)
            }
            TyKind::Error => return self.infer_args_unchecked(args),
            TyKind::Infer(_) => {
                let callee_range = self.expr_anchor(callee);
                self.diag(
                    callee_range,
                    DiagnosticCode::TypeAnnotationsNeeded,
                    "type annotations needed: the type of the called value is unknown",
                );
                return self.infer_args_unchecked(args);
            }
            _ => {
                let callee_range = self.expr_anchor(callee);
                let message = format!("expected a function, found `{}`", self.show(&callee_ty));
                self.diag(callee_range, DiagnosticCode::TypeNotCallable, message);
                return self.infer_args_unchecked(args);
            }
        };
        if params.len() != args.len() {
            self.diag(
                range,
                DiagnosticCode::TypeWrongArity,
                format!(
                    "this function takes {} but {} supplied",
                    plural(params.len(), "argument"),
                    were(args.len())
                ),
            );
        }
        if let Some(expected) = expect.ty() {
            self.try_unify(&ret, expected);
        }
        for (index, &arg) in args.iter().enumerate() {
            match params.get(index) {
                Some(param) => {
                    self.infer_expr(arg, Expect::Coerce(param));
                }
                None => {
                    self.infer_expr(arg, Expect::None);
                }
            }
        }
        self.process_projections();
        if let Some(fn_ref) = &fn_ref
            && self.sigs.std.integer_into.map(FnDefId::Item) == Some(fn_ref.def)
            && let (Some(from), Some(GenericArg::Type(to))) = (&fn_ref.self_ty, fn_ref.args.first())
        {
            self.into_checks.push(IntoCheck {
                from: from.clone(),
                to: to.clone(),
                anchor: range,
            });
        }
        ret
    }

    fn infer_args_unchecked(&mut self, args: &[ExprId]) -> Ty {
        for &arg in args {
            self.infer_expr(arg, Expect::None);
        }
        Ty::error()
    }

    /// `asset::<A>(…)`: the path must be a string literal.
    fn check_asset_path(&mut self, args: &[ExprId]) {
        let Some(&arg) = args.first() else {
            return;
        };
        match self.store.expr(arg) {
            Expr::Literal(Literal::String(_)) | Expr::Missing => {}
            _ => {
                let range = self.expr_anchor(arg);
                self.diag(
                    range,
                    DiagnosticCode::TypeAssetPathNotLiteral,
                    "the path of `asset` must be a string literal",
                );
            }
        }
    }

    /// `nav::start(entry, args, on_exit)`: `entry` names an `#[entry]` scene
    /// `scene fn(P1, …, Pn) -> R`, `args` is `(P1, …, Pn)` and `on_exit` is `Fn(R)`.
    fn infer_nav_start(&mut self, expr: ExprId, callee: ExprId, args: &[ExprId]) -> Ty {
        let range = self.expr_anchor(expr);
        let Some(nav_start) = self.sigs.std.nav_start else {
            return self.infer_args_unchecked(args);
        };
        let [entry, entry_args, on_exit] = args else {
            self.diag(
                range,
                DiagnosticCode::TypeWrongArity,
                format!(
                    "this function takes 3 arguments but {} supplied",
                    were(args.len())
                ),
            );
            self.record_expr(callee, &Ty::error());
            self.result.paths.insert(callee, ValueRef::Error);
            self.infer_args_unchecked(args);
            return Ty::unit();
        };
        let scene = match (self.store.expr(*entry), self.res.store.exprs.get(entry)) {
            (Expr::Path(_), Some(Resolution::Def(Def::Item(item))))
                if self
                    .sigs
                    .fn_sig(*item)
                    .is_some_and(|sig| sig.entry && sig.scene) =>
            {
                Some(*item)
            }
            (_, Some(Resolution::Error)) => None,
            _ => {
                let entry_range = self.expr_anchor(*entry);
                self.diag(
                    entry_range,
                    DiagnosticCode::TypeBadNavStart,
                    "the first argument of `nav::start` must name an `#[entry]` scene",
                );
                None
            }
        };
        let entry_ty = self.infer_expr(*entry, Expect::None);
        let (entry_ptr, args_ty, ret) = match scene {
            Some(_) => match self.structurally_resolve(&entry_ty).kind() {
                TyKind::FnDef(fn_ref) => match self.fn_shape(fn_ref) {
                    Some(shape) => {
                        let ptr = Ty::new(TyKind::FnPtr {
                            kind: FnPtrKind::Scene,
                            params: shape.params.clone().into_boxed_slice(),
                            ret: shape.ret.clone(),
                        });
                        (ptr, Ty::tuple(shape.params), shape.ret)
                    }
                    None => (Ty::error(), Ty::error(), Ty::error()),
                },
                _ => (Ty::error(), Ty::error(), Ty::error()),
            },
            None => (Ty::error(), Ty::error(), Ty::error()),
        };
        self.coerce(*entry, &entry_ty, &entry_ptr);
        self.infer_expr(*entry_args, Expect::Coerce(&args_ty));
        let on_exit_ty = if ret.is_error() {
            Ty::error()
        } else {
            Ty::new(TyKind::Closure {
                kind: ClosureKind::Plain,
                params: Box::new([ret.clone()]),
                ret: Ty::unit(),
            })
        };
        self.infer_expr(*on_exit, Expect::Coerce(&on_exit_ty));
        let fn_ref = FnRef {
            def: FnDefId::Item(nav_start),
            self_ty: None,
            args: crate::ty::GenericArgs::new(vec![
                GenericArg::Type(entry_ptr),
                GenericArg::Type(args_ty),
                GenericArg::Type(ret),
            ]),
        };
        self.record_expr(callee, &Ty::new(TyKind::FnDef(fn_ref.clone())));
        self.result.paths.insert(callee, ValueRef::Fn(fn_ref));
        Ty::unit()
    }

    /// Dereferences `ty` through references: the base type and the number of derefs.
    fn autoderef(&mut self, ty: &Ty) -> (Ty, u32) {
        let mut base = self.structurally_resolve(ty);
        let mut derefs = 0u32;
        while let TyKind::Ref { inner, .. } = base.kind() {
            let inner = inner.clone();
            base = self.structurally_resolve(&inner);
            derefs = derefs.saturating_add(1);
        }
        (base, derefs)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn infer_method_call(
        &mut self,
        expr: ExprId,
        receiver: ExprId,
        name: &Name,
        generic_args: Option<&GenericArgs>,
        args: &[ExprId],
        expect: Expect<'_>,
    ) -> Ty {
        let range = self.expr_anchor(expr);
        let receiver_ty = self.infer_expr(receiver, Expect::None);
        for arg in generic_args.map_or(&[][..], |args| args.0.as_slice()) {
            match *arg {
                crate::hir::GenericArg::Type(ty) => self.infer_type_exprs(ty),
                crate::hir::GenericArg::Const(value) => {
                    self.infer_expr(value, Expect::None);
                }
            }
        }
        let (base, derefs) = self.autoderef(&receiver_ty);
        if base.is_error() {
            return self.infer_args_unchecked(args);
        }
        let is_literal = self.table.literal(&base).is_some();
        if matches!(base.kind(), TyKind::Infer(_)) && !is_literal {
            let receiver_range = self.expr_anchor(receiver);
            self.diag(
                receiver_range,
                DiagnosticCode::TypeAnnotationsNeeded,
                format!("type annotations needed: cannot call `{name}` on a value of unknown type"),
            );
            return self.infer_args_unchecked(args);
        }
        let Some(candidate) = self.lookup_method(&base, name, is_literal) else {
            let message = format!("no method named `{name}` found for `{}`", self.show(&base));
            self.diag(range, DiagnosticCode::TypeUnknownMethod, message);
            return self.infer_args_unchecked(args);
        };
        let (item, fn_ref) = match candidate {
            MethodCandidate::Inherent { item, impl_item } => {
                let impl_args = self.fresh_args_of(impl_item);
                let self_ty = match self.sigs.get(impl_item) {
                    Some(ItemSig::Impl { self_ty, .. }) => subst_ty(
                        self_ty,
                        &Subst {
                            owners: &[(impl_item, 0)],
                            args: &impl_args,
                            self_ty: None,
                        },
                    ),
                    _ => Ty::error(),
                };
                if !self.try_unify(&self_ty, &base) {
                    let message =
                        format!("no method named `{name}` found for `{}`", self.show(&base));
                    self.diag(range, DiagnosticCode::TypeUnknownMethod, message);
                    return self.infer_args_unchecked(args);
                }
                (
                    item,
                    self.fn_ref(item, None, Some(impl_args), generic_args, range),
                )
            }
            MethodCandidate::Trait { item, trait_item } => {
                let trait_args = self.fresh_args_of(trait_item);
                (
                    item,
                    self.fn_ref(
                        item,
                        Some(base.clone()),
                        Some(trait_args),
                        generic_args,
                        range,
                    ),
                )
            }
        };
        let Some(sig) = self.sigs.fn_sig(item) else {
            return self.infer_args_unchecked(args);
        };
        let autoref = match sig.self_param {
            Some(SelfParamKind::Value) => None,
            Some(SelfParamKind::Ref) => Some(false),
            Some(SelfParamKind::RefMut) => Some(true),
            None => {
                let message = format!(
                    "`{name}` is an associated function, not a method; call it as `{}::{name}(…)`",
                    self.show(&base)
                );
                self.diag(range, DiagnosticCode::TypeNotAMethod, message);
                return self.infer_args_unchecked(args);
            }
        };
        self.add_fn_obligations(&fn_ref, range);
        self.result.method_calls.insert(
            expr,
            MethodCall {
                func: fn_ref.clone(),
                receiver: ReceiverAdjust { derefs, autoref },
            },
        );
        let Some(shape) = self.fn_shape(&fn_ref) else {
            return self.infer_args_unchecked(args);
        };
        let params = shape.params.get(1..).unwrap_or(&[]).to_vec();
        if params.len() != args.len() {
            self.diag(
                range,
                DiagnosticCode::TypeWrongArity,
                format!(
                    "this method takes {} but {} supplied",
                    plural(params.len(), "argument"),
                    were(args.len())
                ),
            );
        }
        if let Some(expected) = expect.ty() {
            self.try_unify(&shape.ret, expected);
        }
        for (index, &arg) in args.iter().enumerate() {
            match params.get(index) {
                Some(param) => {
                    self.infer_expr(arg, Expect::Coerce(param));
                }
                None => {
                    self.infer_expr(arg, Expect::None);
                }
            }
        }
        self.process_projections();
        if Some(item) == self.sigs.std.integer_into
            && let Some(GenericArg::Type(to)) = fn_ref.args.first()
        {
            self.into_checks.push(IntoCheck {
                from: base,
                to: to.clone(),
                anchor: range,
            });
        }
        shape.ret
    }

    fn lookup_method(&self, base: &Ty, name: &Name, is_literal: bool) -> Option<MethodCandidate> {
        let is_fn = |item: ItemId| {
            matches!(
                item_tree(self.db, item.file)
                    .item(item.index)
                    .map(|data| &data.kind),
                Some(ItemKind::Fn { .. })
            )
        };
        if let Some(key) = type_key(base)
            && let Some(&item) = self
                .maps
                .inherent
                .get(&key)
                .and_then(|items| items.get(name))
            && is_fn(item)
            && let Some(impl_item) = assoc_parent(self.db, item)
        {
            return Some(MethodCandidate::Inherent { item, impl_item });
        }
        let traits: Vec<ItemId> = match base.kind() {
            TyKind::Param { owner, index } => self.env.bounds_of(*owner, *index).to_vec(),
            TyKind::TraitSelf { trait_item } => vec![*trait_item],
            _ if is_literal => self.sigs.std.integer.into_iter().collect(),
            _ => {
                let mut traits = Vec::new();
                if let Some(key) = type_key(base) {
                    for impl_item in self.maps.trait_impls.get(&key).into_iter().flatten() {
                        if let Some(&trait_item) = self.maps.impl_traits.get(impl_item)
                            && !traits.contains(&trait_item)
                        {
                            traits.push(trait_item);
                        }
                    }
                }
                traits
            }
        };
        traits.into_iter().find_map(|trait_item| {
            let item = trait_assoc(self.db, trait_item, name)?;
            is_fn(item).then_some(MethodCandidate::Trait { item, trait_item })
        })
    }

    pub(crate) fn infer_field(&mut self, expr: ExprId, base: ExprId, name: &FieldName) -> Ty {
        let range = self.expr_anchor(expr);
        let base_ty = self.infer_expr(base, Expect::None);
        let (resolved, derefs) = self.autoderef(&base_ty);
        let found: Option<(u32, Ty)> = match (resolved.kind(), name) {
            (TyKind::Error, _) => return Ty::error(),
            (TyKind::Infer(_), _) => {
                self.diag(
                    range,
                    DiagnosticCode::TypeAnnotationsNeeded,
                    "type annotations needed: cannot access a field of a value of unknown type",
                );
                return Ty::error();
            }
            (TyKind::Tuple(fields), FieldName::Index(index)) => fields
                .get(*index as usize)
                .map(|field| (*index, field.clone())),
            (TyKind::Adt { item, args }, _) => self.sigs.adt(*item).and_then(|adt| {
                if adt.is_enum {
                    return None;
                }
                let variant = adt.variants.first()?;
                let index = match name {
                    FieldName::Named(name) => variant
                        .fields
                        .iter()
                        .position(|field| field.name.as_ref() == Some(name))?,
                    FieldName::Index(index) => {
                        let index = *index as usize;
                        (variant.fields.get(index)?.name.is_none()).then_some(index)?
                    }
                };
                let field = variant.fields.get(index)?;
                let ty = subst_ty(
                    &field.ty,
                    &Subst {
                        owners: &[(*item, 0)],
                        args,
                        self_ty: None,
                    },
                );
                Some((u32::try_from(index).ok()?, ty))
            }),
            _ => None,
        };
        match found {
            Some((index, ty)) => {
                self.result.fields.insert(expr, FieldRef { derefs, index });
                self.normalize(&ty)
            }
            None => {
                let field = match name {
                    FieldName::Named(name) => name.to_string(),
                    FieldName::Index(index) => index.to_string(),
                };
                let message = format!("no field `{field}` on type `{}`", self.show(&resolved));
                self.diag(range, DiagnosticCode::TypeUnknownField, message);
                Ty::error()
            }
        }
    }

    pub(crate) fn infer_index(&mut self, expr: ExprId, base: ExprId, index: ExprId) -> Ty {
        let base_ty = self.infer_expr(base, Expect::None);
        let index_ty = self.infer_expr(index, Expect::None);
        if self.table.literal(&index_ty).is_some() {
            self.table
                .unify(&index_ty, &Ty::int(crate::ty::IntTy::Usize));
        }
        let index_resolved = self.structurally_resolve(&index_ty);
        let is_integer = matches!(
            index_resolved.kind(),
            TyKind::Int(_) | TyKind::Bounded { .. } | TyKind::Error
        );
        if !is_integer {
            let range = self.expr_anchor(index);
            let message = format!(
                "expected an integer index, found `{}`",
                self.show(&index_ty)
            );
            self.diag(range, DiagnosticCode::TypeMismatch, message);
        }
        let (resolved, _) = self.autoderef(&base_ty);
        match resolved.kind() {
            TyKind::Error => Ty::error(),
            TyKind::Array { elem, .. } => elem.clone(),
            TyKind::Adt { item, args } if Some(*item) == self.sigs.std.vec => match args.first() {
                Some(GenericArg::Type(elem)) => elem.clone(),
                _ => Ty::error(),
            },
            _ => {
                let range = self.expr_anchor(expr);
                let message = format!(
                    "cannot index into a value of type `{}`",
                    self.show(&resolved)
                );
                self.diag(range, DiagnosticCode::TypeNotIndexable, message);
                Ty::error()
            }
        }
    }
}
