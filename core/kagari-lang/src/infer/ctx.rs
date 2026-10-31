//! The inference context of one body: expectations, coercions, joins, instantiation of
//! generic items, projections and deferred checks.

use rustc_hash::FxHashMap;

use super::table::Table;
use super::{Coercion, InferenceResult};
use crate::db::Db;
use crate::def_map::{DefMaps, ModuleId};
use crate::diagnostic::{Anchor, Diag, DiagnosticCode, NodeId, Owner, Severity};
use crate::hir::{ClosureKind, Expr, ExprId, ExprStore, FnPtrKind, Name, PatId};
use crate::input::Program;
use crate::item_tree::{FnKind, ItemId};
use crate::resolve::{BodyResolution, LocalId};
use crate::signatures::lower::{Scope, Sink, TyLowerer};
use crate::signatures::subst::{Subst, subst_ty};
use crate::signatures::{
    BoundEnv, GenericParamDefKind, Generics, ItemSig, ProgramSignatures, display_ty_with,
    normalize_projection,
};
use crate::ty::{Const, FnDefId, FnRef, GenericArg, GenericArgs, IntTy, Ty, TyKind};

/// What the context of an expression asks of its type.
#[derive(Clone, Copy)]
pub(crate) enum Expect<'t> {
    None,
    /// A type the expression is likely to have; used to guide inference only.
    Hint(&'t Ty),
    /// A coercion site: the expression's type must coerce to this one.
    Coerce(&'t Ty),
}

impl<'t> Expect<'t> {
    pub(crate) fn ty(self) -> Option<&'t Ty> {
        match self {
            Expect::None => None,
            Expect::Hint(ty) | Expect::Coerce(ty) => Some(ty),
        }
    }
}

pub(crate) struct LoopState {
    /// The type of `break` values; `None` for `while` and `for`.
    pub ty: Option<Ty>,
    pub has_break: bool,
}

pub(crate) struct PendingProjection {
    pub var: Ty,
    pub base: Ty,
    pub trait_item: ItemId,
    pub name: Name,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ObligationKind {
    Bound,
    /// The operands of `==` / `!=`.
    Eq,
}

pub(crate) struct Obligation {
    pub ty: Ty,
    pub trait_item: ItemId,
    pub anchor: Anchor,
    pub kind: ObligationKind,
}

pub(crate) struct IntoCheck {
    pub from: Ty,
    pub to: Ty,
    pub anchor: Anchor,
}

/// An integer literal of a pattern (or a bound of a range pattern) and the matched type.
pub(crate) struct PatFit {
    pub value: i128,
    pub ty: Ty,
    pub anchor: Anchor,
}

/// The signature of a callable, instantiated.
pub(crate) struct FnShape {
    pub params: Vec<Ty>,
    pub ret: Ty,
    pub kind: FnPtrKind,
}

pub(crate) struct InferCtx<'a> {
    pub db: &'a dyn Db,
    pub program: Program,
    pub sigs: &'a ProgramSignatures,
    pub maps: &'a DefMaps,
    pub item: ItemId,
    pub store: &'a ExprStore,
    pub res: &'a BodyResolution,
    pub env: BoundEnv,
    pub lowerer: TyLowerer<'a>,
    pub table: Table,
    pub result: InferenceResult,
    pub diags: Vec<Diag>,
    /// The kind closures get without an expected type.
    pub fn_kind: ClosureKind,
    pub closure_kinds: Vec<ClosureKind>,
    /// The return type of the innermost function or closure.
    pub returns: Vec<Ty>,
    pub loops: FxHashMap<ExprId, LoopState>,
    pub modules: Vec<ModuleId>,
    /// The impl or trait `Self` refers to.
    pub self_owner: Option<ItemId>,
    /// Generic parameters in scope: `(name, owner, index, is_const)`.
    pub generic_scope: Vec<(Name, ItemId, u32, bool)>,
    pub projections: Vec<PendingProjection>,
    /// Groups of literal variables that end up with the same Rust integer type.
    pub links: Vec<Vec<Ty>>,
    pub obligations: Vec<Obligation>,
    pub into_checks: Vec<IntoCheck>,
    pub pat_fits: Vec<PatFit>,
}

impl<'a> InferCtx<'a> {
    pub(crate) fn diag(
        &mut self,
        anchor: Anchor,
        code: DiagnosticCode,
        message: impl Into<String>,
    ) {
        self.diags.push(Diag::new(anchor, code, message));
    }

    pub(crate) fn has_errors(&self) -> bool {
        self.diags
            .iter()
            .any(|diag| diag.severity() == Severity::Error)
    }

    pub(crate) fn expr_anchor(&self, expr: ExprId) -> Anchor {
        Anchor::Node(Owner::Body(self.item), NodeId::Expr(expr))
    }

    pub(crate) fn pat_anchor(&self, pat: PatId) -> Anchor {
        Anchor::Node(Owner::Body(self.item), NodeId::Pat(pat))
    }

    /// The diagnostic text of `ty` as currently inferred.
    pub(crate) fn show(&self, ty: &Ty) -> String {
        let resolved = self.table.resolve(ty);
        display_ty_with(self.db, Some(self.sigs), &resolved, &|var| {
            self.table.is_literal_var(var)
        })
    }

    pub(crate) fn mismatch(&mut self, range: Anchor, expected: &Ty, found: &Ty) {
        let expected_resolved = self.table.resolve(expected);
        let found_resolved = self.table.resolve(found);
        if expected_resolved.references_error() || found_resolved.references_error() {
            return;
        }
        let message = format!(
            "mismatched types: expected `{}`, found `{}`",
            self.show(expected),
            self.show(found)
        );
        self.diag(range, DiagnosticCode::TypeMismatch, message);
    }

    pub(crate) fn record_expr(&mut self, expr: ExprId, ty: &Ty) {
        self.result.expr_types.insert(expr, ty.clone());
    }

    pub(crate) fn record_pat(&mut self, pat: PatId, ty: &Ty) {
        self.result.pat_types.insert(pat, ty.clone());
    }

    pub(crate) fn local_ty(&mut self, local: LocalId) -> Ty {
        if let Some(ty) = self.result.local_types.get(local) {
            return ty.clone();
        }
        let ty = self.table.new_var();
        self.result.local_types.insert(local, ty.clone());
        ty
    }

    pub(crate) fn current_module(&self) -> ModuleId {
        self.modules
            .last()
            .copied()
            .unwrap_or_else(|| self.maps.module_of_item(self.db, self.item))
    }

    pub(crate) fn scope(&self) -> Scope<'a> {
        Scope {
            src: Owner::Body(self.item),
            store: self.store,
            res: &self.res.store,
        }
    }

    /// Lowers a type written in the body; `_` becomes an inference variable.
    pub(crate) fn lower_type(&mut self, ty: crate::hir::TypeRefId) -> Ty {
        let scope = self.scope();
        let table = &mut self.table;
        let mut hole = |is_const: bool| {
            if is_const {
                GenericArg::Const(table.new_const())
            } else {
                GenericArg::Type(table.new_var())
            }
        };
        let mut sink = Sink {
            diags: Vec::new(),
            quiet: false,
            hole: Some(&mut hole),
            infer_args: false,
        };
        let lowered = self.lowerer.lower(scope, ty, &mut sink);
        let diags = sink.diags;
        self.diags.extend(diags);
        self.infer_type_exprs(ty);
        lowered
    }

    /// Gives the constant expressions inside a written type (array lengths, const generic
    /// arguments) their types.
    pub(crate) fn infer_type_exprs(&mut self, ty: crate::hir::TypeRefId) {
        let store = self.store;
        match store.type_ref(ty) {
            crate::hir::TypeRef::Path(path) => self.infer_path_arg_exprs(path),
            crate::hir::TypeRef::Tuple(fields) => {
                for &field in fields {
                    self.infer_type_exprs(field);
                }
            }
            crate::hir::TypeRef::Array { elem, len } => {
                self.infer_type_exprs(*elem);
                self.infer_expr(
                    *len,
                    super::ctx::Expect::Coerce(&Ty::int(crate::ty::IntTy::Usize)),
                );
            }
            crate::hir::TypeRef::Ref { inner, .. } => self.infer_type_exprs(*inner),
            crate::hir::TypeRef::FnPtr { params, ret, .. }
            | crate::hir::TypeRef::Closure { params, ret, .. } => {
                for &param in params {
                    self.infer_type_exprs(param);
                }
                if let Some(ret) = ret {
                    self.infer_type_exprs(*ret);
                }
            }
            crate::hir::TypeRef::Missing | crate::hir::TypeRef::Infer => {}
        }
    }

    /// Gives the constant generic arguments of a path their types.
    pub(crate) fn infer_path_arg_exprs(&mut self, path: &crate::hir::Path) {
        for segment in &path.segments {
            let Some(args) = &segment.args else {
                continue;
            };
            for arg in &args.0 {
                match *arg {
                    crate::hir::GenericArg::Type(ty) => self.infer_type_exprs(ty),
                    crate::hir::GenericArg::Const(expr) => {
                        self.infer_expr(expr, super::ctx::Expect::None);
                    }
                }
            }
        }
    }

    /// Runs `f` with a lowering sink whose holes and omitted generic arguments are inference
    /// variables.
    pub(crate) fn with_expr_sink<R>(
        &mut self,
        f: impl FnOnce(&mut TyLowerer<'a>, Scope<'a>, &mut Sink<'_>) -> R,
    ) -> R {
        let scope = self.scope();
        let table = &mut self.table;
        let mut hole = |is_const: bool| {
            if is_const {
                GenericArg::Const(table.new_const())
            } else {
                GenericArg::Type(table.new_var())
            }
        };
        let mut sink = Sink {
            diags: Vec::new(),
            quiet: false,
            hole: Some(&mut hole),
            infer_args: true,
        };
        let out = f(&mut self.lowerer, scope, &mut sink);
        let diags = sink.diags;
        self.diags.extend(diags);
        out
    }

    pub(crate) fn fresh_args(&mut self, generics: &Generics) -> Vec<GenericArg> {
        generics
            .params
            .iter()
            .map(|param| match param.kind {
                GenericParamDefKind::Type { .. } => GenericArg::Type(self.table.new_var()),
                GenericParamDefKind::Const { .. } => GenericArg::Const(self.table.new_const()),
            })
            .collect()
    }

    /// Fresh arguments for the parameters of `owner` (an impl, trait or ADT).
    pub(crate) fn fresh_args_of(&mut self, owner: ItemId) -> Vec<GenericArg> {
        match self.sigs.generics(owner) {
            Some(generics) => self.fresh_args(generics),
            None => Vec::new(),
        }
    }

    /// `ty` with its head resolved; pending projections are processed first when it is an
    /// unresolved variable.
    pub(crate) fn structurally_resolve(&mut self, ty: &Ty) -> Ty {
        let shallow = self.table.shallow(ty);
        if matches!(shallow.kind(), TyKind::Infer(_)) && !self.projections.is_empty() {
            self.process_projections();
            return self.table.shallow(ty);
        }
        shallow
    }

    // ---- coercion ----

    /// Coerces `actual`, the type of `expr`, to `target`, recording the coercion or reporting
    /// a mismatch.
    pub(crate) fn coerce(&mut self, expr: ExprId, actual: &Ty, target: &Ty) -> bool {
        let a = self.structurally_resolve(actual);
        let t = self.structurally_resolve(target);
        match (a.kind(), t.kind()) {
            (TyKind::Error, _) | (_, TyKind::Error) | (TyKind::Never, TyKind::Never) => {
                return true;
            }
            (TyKind::Never, _) => {
                self.result
                    .coercions
                    .insert(expr, (Coercion::NeverToAny, t.clone()));
                return true;
            }
            _ => {}
        }
        match self.try_coerce(expr, &a, &t) {
            Some(coercion) => {
                if let Some(coercion) = coercion {
                    self.result.coercions.insert(expr, (coercion, t.clone()));
                }
                true
            }
            None => {
                let range = self.expr_anchor(expr);
                self.mismatch(range, &t, &a);
                false
            }
        }
    }

    /// `Some(None)` when the types unify, `Some(Some(coercion))` when a coercion applies.
    fn try_coerce(&mut self, expr: ExprId, a: &Ty, t: &Ty) -> Option<Option<Coercion>> {
        let snapshot = self.table.snapshot();
        if self.table.unify(a, t) {
            return Some(None);
        }
        self.table.rollback(snapshot);
        match (a.kind(), t.kind()) {
            (TyKind::Bounded { lo: l1, hi: h1 }, TyKind::Bounded { lo: l2, hi: h2 }) => {
                let values = (
                    self.table.shallow_const(l1).value(),
                    self.table.shallow_const(h1).value(),
                    self.table.shallow_const(l2).value(),
                    self.table.shallow_const(h2).value(),
                );
                match values {
                    (Some(l1), Some(h1), Some(l2), Some(h2)) if l2 <= l1 && h1 <= h2 => {
                        Some(Some(Coercion::IntWiden))
                    }
                    _ => None,
                }
            }
            (TyKind::FnDef(fn_ref), TyKind::FnPtr { kind, params, ret }) => {
                let shape = self.fn_shape(fn_ref)?;
                if shape.kind != *kind {
                    return None;
                }
                self.unify_sig_or_rollback(&shape.params, &shape.ret, params, ret)
                    .then_some(Some(Coercion::FnDefToPtr))
            }
            (TyKind::FnDef(fn_ref), TyKind::Closure { kind, params, ret }) => {
                let shape = self.fn_shape(fn_ref)?;
                if ptr_to_closure_kind(shape.kind) != Some(*kind) {
                    return None;
                }
                self.unify_sig_or_rollback(&shape.params, &shape.ret, params, ret)
                    .then_some(Some(Coercion::ToClosure))
            }
            (
                TyKind::FnPtr {
                    kind: k1,
                    params: p1,
                    ret: r1,
                },
                TyKind::Closure {
                    kind: k2,
                    params: p2,
                    ret: r2,
                },
            ) => {
                if ptr_to_closure_kind(*k1) != Some(*k2) {
                    return None;
                }
                self.unify_sig_or_rollback(p1, r1, p2, r2)
                    .then_some(Some(Coercion::ToClosure))
            }
            (
                TyKind::Closure {
                    kind: k1,
                    params: p1,
                    ret: r1,
                },
                TyKind::FnPtr {
                    kind: k2,
                    params: p2,
                    ret: r2,
                },
            ) => {
                if ptr_to_closure_kind(*k2) != Some(*k1) {
                    return None;
                }
                if !self.unify_sig_or_rollback(p1, r1, p2, r2) {
                    return None;
                }
                let captures = match self.store.expr(expr) {
                    Expr::Closure { .. } => self
                        .result
                        .closures
                        .get(&expr)
                        .map(|info| info.captures.clone()),
                    _ => None,
                };
                match captures {
                    Some(captures) if captures.is_empty() => Some(Some(Coercion::ClosureToPtr)),
                    Some(captures) => {
                        let names: Vec<String> = captures
                            .iter()
                            .map(|local| format!("`{}`", self.res.locals[*local].name))
                            .collect();
                        let range = self.expr_anchor(expr);
                        self.diag(
                            range,
                            DiagnosticCode::TypeCapturingClosure,
                            format!(
                                "this closure captures {}, so it cannot become a function pointer",
                                names.join(", ")
                            ),
                        );
                        Some(None)
                    }
                    None => None,
                }
            }
            (
                TyKind::Ref {
                    mutable: true,
                    inner: i1,
                },
                TyKind::Ref {
                    mutable: false,
                    inner: i2,
                },
            ) => {
                let snapshot = self.table.snapshot();
                if self.table.unify(i1, i2) {
                    Some(Some(Coercion::MutToShared))
                } else {
                    self.table.rollback(snapshot);
                    None
                }
            }
            _ => None,
        }
    }

    fn unify_sig_or_rollback(&mut self, p1: &[Ty], r1: &Ty, p2: &[Ty], r2: &Ty) -> bool {
        let snapshot = self.table.snapshot();
        if self.table.unify_sig(p1, r1, p2, r2) {
            true
        } else {
            self.table.rollback(snapshot);
            false
        }
    }

    /// Unifies `a` with `b`, undoing partial bindings when they do not unify.
    pub(crate) fn try_unify(&mut self, a: &Ty, b: &Ty) -> bool {
        let snapshot = self.table.snapshot();
        if self.table.unify(a, b) {
            true
        } else {
            self.table.rollback(snapshot);
            false
        }
    }

    /// The concrete bounds of an `Int` type.
    pub(crate) fn int_range(&self, ty: &Ty) -> Option<(i128, i128)> {
        match self.table.shallow(ty).kind() {
            TyKind::Bounded { lo, hi } => Some((
                self.table.shallow_const(lo).value()?,
                self.table.shallow_const(hi).value()?,
            )),
            TyKind::Int(int) => Some((IntTy::min(*int), IntTy::max(*int))),
            _ => None,
        }
    }

    pub(crate) fn bounded_range(&self, ty: &Ty) -> Option<(i128, i128)> {
        match self.table.shallow(ty).kind() {
            TyKind::Bounded { .. } => self.int_range(ty),
            _ => None,
        }
    }

    /// Joins the types of the arms of an `if` or `match` without an expected type: `!` arms
    /// are ignored, concrete `Int`s join to their hull, anything else must agree with the
    /// first arm.
    pub(crate) fn join_arms(&mut self, arms: &[(ExprId, Ty)]) -> Ty {
        let mut joined: Option<Ty> = None;
        for (expr, ty) in arms {
            let shallow = self.structurally_resolve(ty);
            if shallow.is_never() {
                continue;
            }
            joined = Some(match joined {
                None => ty.clone(),
                Some(acc) => self.join2(*expr, &acc, ty),
            });
        }
        let Some(joined) = joined else {
            return Ty::never();
        };
        for (expr, ty) in arms {
            let shallow = self.table.shallow(ty);
            if shallow.is_never() {
                if !self.table.shallow(&joined).is_never() {
                    self.result
                        .coercions
                        .insert(*expr, (Coercion::NeverToAny, joined.clone()));
                }
                continue;
            }
            if let (Some(own), Some(all)) = (self.bounded_range(ty), self.bounded_range(&joined))
                && own != all
            {
                self.result
                    .coercions
                    .insert(*expr, (Coercion::IntWiden, joined.clone()));
            }
        }
        joined
    }

    fn join2(&mut self, expr: ExprId, acc: &Ty, ty: &Ty) -> Ty {
        if let Some((_, literal)) = self.table.literal(ty)
            && self.bounded_range(acc).is_some()
        {
            self.table.unify(ty, &Ty::bounded(literal.lo, literal.hi));
        }
        if let Some((_, literal)) = self.table.literal(acc)
            && self.bounded_range(ty).is_some()
        {
            self.table.unify(acc, &Ty::bounded(literal.lo, literal.hi));
        }
        if let (Some((l1, h1)), Some((l2, h2))) = (self.bounded_range(acc), self.bounded_range(ty))
        {
            return Ty::bounded(l1.min(l2), h1.max(h2));
        }
        if self.try_unify(acc, ty) {
            return acc.clone();
        }
        self.coerce(expr, ty, acc);
        acc.clone()
    }

    // ---- generic items ----

    /// The owners and offsets of the generic arguments of an item: its parent's first.
    pub(crate) fn owners_of(&self, item: ItemId) -> Vec<(ItemId, usize)> {
        let mut owners = Vec::new();
        if let Some(generics) = self.sigs.generics(item) {
            if let Some(parent) = generics.parent {
                owners.push((parent, 0));
            }
            owners.push((item, generics.parent_count as usize));
        }
        owners
    }

    pub(crate) fn fn_def_kind(&self, def: FnDefId) -> FnPtrKind {
        match def {
            FnDefId::Item(item) => match self.sigs.fn_sig(item) {
                Some(sig) => fn_ptr_kind(sig.kind, sig.scene),
                None => FnPtrKind::Plain,
            },
            FnDefId::StructCtor(_) | FnDefId::VariantCtor(..) => FnPtrKind::Const,
        }
    }

    /// The instantiated signature of a function or tuple constructor.
    pub(crate) fn fn_shape(&mut self, fn_ref: &FnRef) -> Option<FnShape> {
        let kind = self.fn_def_kind(fn_ref.def);
        let (params, ret) = match fn_ref.def {
            FnDefId::Item(item) => {
                let sig = self.sigs.fn_sig(item)?;
                let owners = self.owners_of(item);
                let subst = Subst {
                    owners: &owners,
                    args: &fn_ref.args,
                    self_ty: fn_ref.self_ty.as_ref(),
                };
                let params: Vec<Ty> = sig
                    .params
                    .iter()
                    .map(|param| subst_ty(param, &subst))
                    .collect();
                let ret = subst_ty(&sig.ret, &subst);
                (params, ret)
            }
            FnDefId::StructCtor(adt) | FnDefId::VariantCtor(adt, _) => {
                let variant = match fn_ref.def {
                    FnDefId::VariantCtor(_, index) => index as usize,
                    _ => 0,
                };
                let def = self.sigs.adt(adt)?;
                let subst = Subst {
                    owners: &[(adt, 0)],
                    args: &fn_ref.args,
                    self_ty: None,
                };
                let params = def
                    .variants
                    .get(variant)?
                    .fields
                    .iter()
                    .map(|field| subst_ty(&field.ty, &subst))
                    .collect();
                let ret = Ty::adt(adt, fn_ref.args.clone());
                (params, ret)
            }
        };
        let params = params.iter().map(|param| self.normalize(param)).collect();
        let ret = self.normalize(&ret);
        Some(FnShape { params, ret, kind })
    }

    /// Records the trait bounds of the generic arguments of `fn_ref` (and `Self: Trait` for a
    /// method declared in a trait), checked once the body is solved.
    pub(crate) fn add_fn_obligations(&mut self, fn_ref: &FnRef, range: Anchor) {
        let FnDefId::Item(item) = fn_ref.def else {
            return;
        };
        let owners = self.owners_of(item);
        for (owner, offset) in owners {
            let Some(generics) = self.sigs.generics(owner) else {
                continue;
            };
            for (index, param) in generics.params.iter().enumerate() {
                let GenericParamDefKind::Type { bounds } = &param.kind else {
                    continue;
                };
                let Some(GenericArg::Type(arg)) = fn_ref.args.get(offset.saturating_add(index))
                else {
                    continue;
                };
                for &bound in bounds {
                    self.obligations.push(Obligation {
                        ty: arg.clone(),
                        trait_item: bound,
                        anchor: range,
                        kind: ObligationKind::Bound,
                    });
                }
            }
        }
        if let Some(self_ty) = &fn_ref.self_ty
            && let Some(parent) = self
                .sigs
                .generics(item)
                .and_then(|generics| generics.parent)
            && matches!(self.sigs.get(parent), Some(ItemSig::Trait { .. }))
        {
            self.obligations.push(Obligation {
                ty: self_ty.clone(),
                trait_item: parent,
                anchor: range,
                kind: ObligationKind::Bound,
            });
        }
    }

    // ---- projections ----

    /// `ty` with its projections normalized, or replaced by variables until their base is
    /// known.
    pub(crate) fn normalize(&mut self, ty: &Ty) -> Ty {
        match ty.kind() {
            TyKind::Projection {
                base,
                trait_item,
                name,
            } => {
                let base = self.normalize(base);
                self.normalize_projection(&base, *trait_item, name)
            }
            TyKind::Tuple(fields) => Ty::new(TyKind::Tuple(
                fields.iter().map(|field| self.normalize(field)).collect(),
            )),
            TyKind::Array { elem, len } => Ty::new(TyKind::Array {
                elem: self.normalize(elem),
                len: len.clone(),
            }),
            TyKind::Adt { item, args } => Ty::new(TyKind::Adt {
                item: *item,
                args: args
                    .iter()
                    .map(|arg| match arg {
                        GenericArg::Type(ty) => GenericArg::Type(self.normalize(ty)),
                        GenericArg::Const(c) => GenericArg::Const(c.clone()),
                    })
                    .collect::<GenericArgs>(),
            }),
            TyKind::Ref { mutable, inner } => Ty::new(TyKind::Ref {
                mutable: *mutable,
                inner: self.normalize(inner),
            }),
            TyKind::FnPtr { kind, params, ret } => Ty::new(TyKind::FnPtr {
                kind: *kind,
                params: params.iter().map(|param| self.normalize(param)).collect(),
                ret: self.normalize(ret),
            }),
            TyKind::Closure { kind, params, ret } => Ty::new(TyKind::Closure {
                kind: *kind,
                params: params.iter().map(|param| self.normalize(param)).collect(),
                ret: self.normalize(ret),
            }),
            _ => ty.clone(),
        }
    }

    fn normalize_projection(&mut self, base: &Ty, trait_item: ItemId, name: &Name) -> Ty {
        match self.table.shallow(base).kind() {
            TyKind::Infer(_) => {
                let var = self.table.new_var();
                self.projections.push(PendingProjection {
                    var: var.clone(),
                    base: base.clone(),
                    trait_item,
                    name: name.clone(),
                });
                var
            }
            TyKind::Error => Ty::error(),
            TyKind::Param { .. } | TyKind::TraitSelf { .. } => Ty::new(TyKind::Projection {
                base: self.table.resolve(base),
                trait_item,
                name: name.clone(),
            }),
            _ => {
                let resolved = self.table.resolve(base);
                normalize_projection(self.db, self.sigs, self.maps, &resolved, trait_item, name)
                    .map_or(Ty::error(), |ty| self.normalize(&ty))
            }
        }
    }

    /// Normalizes the pending projections whose base is known now.
    pub(crate) fn process_projections(&mut self) {
        loop {
            let pending = std::mem::take(&mut self.projections);
            let before = pending.len();
            for projection in pending {
                if matches!(
                    self.table.shallow(&projection.base).kind(),
                    TyKind::Infer(_)
                ) {
                    self.projections.push(projection);
                    continue;
                }
                let normalized = self.normalize_projection(
                    &projection.base,
                    projection.trait_item,
                    &projection.name,
                );
                self.table.unify(&projection.var, &normalized);
            }
            if self.projections.len() >= before {
                break;
            }
        }
    }

    // ---- literal links ----

    pub(crate) fn link(&mut self, members: Vec<Ty>) {
        self.links.push(members);
    }

    /// Makes every literal of a link take the Rust integer type of another member, or, when
    /// another member is an `Int`, the `Int` of its own hull: a link's members are all Rust
    /// integers or all `Int`s.
    pub(crate) fn resolve_links(&mut self) {
        loop {
            let mut changed = false;
            let links = std::mem::take(&mut self.links);
            for members in &links {
                let rust_int =
                    members
                        .iter()
                        .find_map(|member| match self.table.shallow(member).kind() {
                            TyKind::Int(int) => Some(*int),
                            _ => None,
                        });
                let bounded = members.iter().any(|member| {
                    matches!(self.table.shallow(member).kind(), TyKind::Bounded { .. })
                });
                if rust_int.is_none() && !bounded {
                    continue;
                }
                for member in members {
                    if let Some((_, literal)) = self.table.literal(member) {
                        let ty = match rust_int {
                            Some(int) => Ty::int(int),
                            None => Ty::bounded(literal.lo, literal.hi),
                        };
                        self.table.unify(member, &ty);
                        changed = true;
                    }
                }
            }
            self.links = links;
            if !changed {
                break;
            }
        }
    }

    pub(crate) fn const_value(&self, c: &Const) -> Option<i128> {
        self.table.shallow_const(c).value()
    }
}

pub(crate) fn fn_ptr_kind(kind: FnKind, scene: bool) -> FnPtrKind {
    match kind {
        FnKind::Plain => FnPtrKind::Plain,
        FnKind::Const => FnPtrKind::Const,
        FnKind::Pushdown if scene => FnPtrKind::Scene,
        FnKind::Pushdown => FnPtrKind::Pushdown,
    }
}

pub(crate) fn ptr_to_closure_kind(kind: FnPtrKind) -> Option<ClosureKind> {
    match kind {
        FnPtrKind::Plain => Some(ClosureKind::Plain),
        FnPtrKind::Const => Some(ClosureKind::Const),
        FnPtrKind::Pushdown => Some(ClosureKind::Pushdown),
        FnPtrKind::Scene => None,
    }
}
