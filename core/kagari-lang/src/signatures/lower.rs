//! Lowering of HIR type references to semantic types, shared by item signatures and bodies.

use crate::diagnostic::{Anchor, Diag, NodeId, Owner};
use rustc_hash::FxHashMap;

use super::consts::{fold_const_expr, fold_const_item};
use super::subst::{Subst, subst_ty};
use crate::db::Db;
use crate::def_map::def_maps;
use crate::def_map::{Def, PrimType, Resolution};
use crate::diagnostic::DiagnosticCode;
use crate::hir::{
    ExprStore, GenericArg as HirGenericArg, GenericArgs, GenericParamKind, Name, Signature,
    SignatureKind, TypeRef, TypeRefId, signature_of,
};
use crate::input::Program;
use crate::item_tree::{ItemId, ItemKind, item_tree};
use crate::resolve::{StoreResolution, signature_resolution};
use crate::ty::{Const, GenericArg, IntTy, Ty, TyKind};

/// The Rust integer type of a primitive, `None` for `bool` and `Int`.
pub(crate) fn prim_int(prim: PrimType) -> Option<IntTy> {
    Some(match prim {
        PrimType::U8 => IntTy::U8,
        PrimType::U16 => IntTy::U16,
        PrimType::U32 => IntTy::U32,
        PrimType::U64 => IntTy::U64,
        PrimType::U128 => IntTy::U128,
        PrimType::Usize => IntTy::Usize,
        PrimType::I8 => IntTy::I8,
        PrimType::I16 => IntTy::I16,
        PrimType::I32 => IntTy::I32,
        PrimType::I64 => IntTy::I64,
        PrimType::I128 => IntTy::I128,
        PrimType::Isize => IntTy::Isize,
        PrimType::Bool | PrimType::Int => return None,
    })
}

pub(crate) fn item_name(db: &dyn Db, item: ItemId) -> Name {
    item_tree(db, item.file)
        .item(item.index)
        .and_then(|data| data.name.clone())
        .unwrap_or_default()
}

/// The HIR signature and its resolution of an item.
pub(crate) fn item_signature(
    db: &dyn Db,
    program: Program,
    item: ItemId,
) -> Option<(&Signature, &StoreResolution)> {
    if !def_maps(db, program).file_modules.contains_key(&item.file) {
        return None;
    }
    let signature = signature_of(db, item)?;
    Some((signature, &signature_resolution(db, program, item).store))
}

/// Where a type reference lives: the expression store `src` (an item's signature or body)
/// and its resolution.
#[derive(Clone, Copy)]
pub(crate) struct Scope<'s> {
    pub src: Owner,
    pub store: &'s ExprStore,
    pub res: &'s StoreResolution,
}

impl Scope<'_> {
    pub(crate) fn node(&self, node: NodeId) -> Anchor {
        Anchor::Node(self.src, node)
    }
}

/// Where lowering reports, and what `_` lowers to.
pub(crate) struct Sink<'h> {
    pub diags: Vec<Diag>,
    /// Diagnostics are dropped: the types are reported where they are declared.
    pub quiet: bool,
    /// Makes a fresh inference variable (a constant one when the flag is set) for `_`;
    /// `None` in signatures, where `_` is an error.
    pub hole: Option<&'h mut dyn FnMut(bool) -> GenericArg>,
    /// Omitted generic arguments of a path become holes instead of an error (expression
    /// paths such as `Vec::new`).
    pub infer_args: bool,
}

impl Sink<'_> {
    pub(crate) fn loud() -> Sink<'static> {
        Sink {
            diags: Vec::new(),
            quiet: false,
            hole: None,
            infer_args: false,
        }
    }

    fn quiet() -> Sink<'static> {
        Sink {
            diags: Vec::new(),
            quiet: true,
            hole: None,
            infer_args: false,
        }
    }

    fn hole_arg(&mut self, is_const: bool) -> Option<GenericArg> {
        self.hole.as_mut().map(|hole| hole(is_const))
    }
}

pub(crate) struct TyLowerer<'a> {
    pub db: &'a dyn Db,
    pub program: Program,
    alias_values: FxHashMap<ItemId, Option<Ty>>,
    impl_selfs: FxHashMap<ItemId, Ty>,
    alias_stack: Vec<ItemId>,
    impl_stack: Vec<ItemId>,
    /// Alias cycles, kept even when the lowering that finds them is quiet.
    pub cycle_diags: Vec<Diag>,
}

impl<'a> TyLowerer<'a> {
    pub(crate) fn new(db: &'a dyn Db, program: Program) -> TyLowerer<'a> {
        TyLowerer {
            db,
            program,
            alias_values: FxHashMap::default(),
            impl_selfs: FxHashMap::default(),
            alias_stack: Vec::new(),
            impl_stack: Vec::new(),
            cycle_diags: Vec::new(),
        }
    }

    fn diag(
        &self,
        sink: &mut Sink<'_>,
        anchor: Anchor,
        code: DiagnosticCode,
        message: impl Into<String>,
    ) {
        if !sink.quiet {
            sink.diags.push(Diag::new(anchor, code, message));
        }
    }

    pub(crate) fn lower(&mut self, scope: Scope<'_>, id: TypeRefId, sink: &mut Sink<'_>) -> Ty {
        let range = scope.node(NodeId::Type(id));
        match scope.store.type_ref(id) {
            TypeRef::Missing => Ty::error(),
            TypeRef::Infer => match sink.hole_arg(false) {
                Some(GenericArg::Type(ty)) => ty,
                Some(GenericArg::Const(_)) => Ty::error(),
                None => {
                    self.diag(
                        sink,
                        range,
                        DiagnosticCode::TypeInferInSignature,
                        "the placeholder `_` is not allowed in item signatures",
                    );
                    Ty::error()
                }
            },
            TypeRef::Path(path) => {
                let Some(resolution) = scope.res.types.get(&id) else {
                    return Ty::error();
                };
                let args = path
                    .segments
                    .last()
                    .and_then(|segment| segment.args.as_ref());
                self.lower_resolution(scope, resolution, args, range, sink)
            }
            TypeRef::Tuple(fields) => Ty::new(TyKind::Tuple(
                fields
                    .iter()
                    .map(|&field| self.lower(scope, field, sink))
                    .collect(),
            )),
            TypeRef::Array { elem, len } => {
                let elem = self.lower(scope, *elem, sink);
                let len = self.fold(scope, *len, sink);
                Ty::new(TyKind::Array { elem, len })
            }
            TypeRef::Ref { mutable, inner } => {
                Ty::reference(*mutable, self.lower(scope, *inner, sink))
            }
            TypeRef::FnPtr { kind, params, ret } => {
                let params = params
                    .iter()
                    .map(|&param| self.lower(scope, param, sink))
                    .collect();
                let ret = match ret {
                    Some(ret) => self.lower(scope, *ret, sink),
                    None => Ty::unit(),
                };
                Ty::new(TyKind::FnPtr {
                    kind: *kind,
                    params,
                    ret,
                })
            }
            TypeRef::Closure { kind, params, ret } => {
                let params = params
                    .iter()
                    .map(|&param| self.lower(scope, param, sink))
                    .collect();
                let ret = match ret {
                    Some(ret) => self.lower(scope, *ret, sink),
                    None => Ty::unit(),
                };
                Ty::new(TyKind::Closure {
                    kind: *kind,
                    params,
                    ret,
                })
            }
        }
    }

    pub(crate) fn fold(
        &mut self,
        scope: Scope<'_>,
        expr: crate::hir::ExprId,
        sink: &mut Sink<'_>,
    ) -> Const {
        let mut diags = Vec::new();
        let value = fold_const_expr(
            self.db,
            self.program,
            scope.src,
            scope.store,
            scope.res,
            expr,
            &mut diags,
        );
        if !sink.quiet {
            sink.diags.extend(diags);
        }
        value
    }

    /// A type path that resolved to `resolution`, with the generic arguments of its last
    /// segment.
    pub(crate) fn lower_resolution(
        &mut self,
        scope: Scope<'_>,
        resolution: &Resolution,
        args: Option<&GenericArgs>,
        range: Anchor,
        sink: &mut Sink<'_>,
    ) -> Ty {
        match resolution {
            Resolution::Prim(prim) => self.lower_prim(scope, *prim, args, range, sink),
            Resolution::Def(Def::Item(item)) => {
                let tree = item_tree(self.db, item.file);
                let Some(data) = tree.item(item.index) else {
                    return Ty::error();
                };
                match &data.kind {
                    ItemKind::Struct { .. } | ItemKind::Enum { .. } => {
                        let args = self.lower_args(scope, *item, args, range, sink);
                        Ty::adt(*item, crate::ty::GenericArgs::new(args))
                    }
                    ItemKind::TypeAlias { .. } => {
                        let args = self.lower_args(scope, *item, args, range, sink);
                        self.expand_alias(*item, &args, range, sink)
                    }
                    kind => {
                        let what = match kind {
                            ItemKind::Trait { .. } => "trait",
                            ItemKind::Fn { .. } => "function",
                            ItemKind::Const { .. } => "constant",
                            ItemKind::Static { .. } => "static",
                            _ => data.kind_name(),
                        };
                        let name = data.name.clone().unwrap_or_default();
                        self.diag(
                            sink,
                            range,
                            DiagnosticCode::TypeExpectedType,
                            format!("expected a type, found {what} `{name}`"),
                        );
                        Ty::error()
                    }
                }
            }
            Resolution::Def(Def::Variant { enum_item, index }) => {
                let variant = match item_tree(self.db, enum_item.file)
                    .item(enum_item.index)
                    .map(|data| &data.kind)
                {
                    Some(ItemKind::Enum { variants }) => variants
                        .get(*index as usize)
                        .map(|(name, _)| name.clone())
                        .unwrap_or_default(),
                    _ => Name::default(),
                };
                self.diag(
                    sink,
                    range,
                    DiagnosticCode::TypeExpectedType,
                    format!("expected a type, found enum variant `{variant}`"),
                );
                Ty::error()
            }
            Resolution::Def(Def::Module(_)) => {
                self.diag(
                    sink,
                    range,
                    DiagnosticCode::TypeExpectedType,
                    "expected a type, found a module",
                );
                Ty::error()
            }
            Resolution::GenericParam { owner, index } => Ty::new(TyKind::Param {
                owner: *owner,
                index: *index,
            }),
            Resolution::SelfType(owner) => self.self_type(*owner),
            Resolution::Assoc(item) => self.assoc_type(*item, range, sink),
            Resolution::Deferred { base, rest } => self.deferred_type(base, rest, range, sink),
            Resolution::Local(_) | Resolution::Error => Ty::error(),
        }
    }

    fn lower_prim(
        &mut self,
        scope: Scope<'_>,
        prim: PrimType,
        args: Option<&GenericArgs>,
        range: Anchor,
        sink: &mut Sink<'_>,
    ) -> Ty {
        if prim != PrimType::Int {
            if args.is_some() {
                self.diag(
                    sink,
                    range,
                    DiagnosticCode::TypeWrongGenericCount,
                    format!("`{}` takes no generic arguments", prim.name()),
                );
            }
            return match prim_int(prim) {
                Some(int) => Ty::int(int),
                None => Ty::bool(),
            };
        }
        let Some(GenericArgs(args)) = args else {
            self.diag(
                sink,
                range,
                DiagnosticCode::TypeWrongGenericCount,
                "`Int` needs two bounds: `Int<lo, hi>`",
            );
            return Ty::error();
        };
        let [lo, hi] = args.as_slice() else {
            self.diag(
                sink,
                range,
                DiagnosticCode::TypeWrongGenericCount,
                format!(
                    "`Int` takes 2 generic arguments but {} were supplied",
                    args.len()
                ),
            );
            return Ty::error();
        };
        let lo = self.lower_const_arg(scope, lo, sink);
        let hi = self.lower_const_arg(scope, hi, sink);
        let i64_range = i128::from(i64::MIN)..=i128::from(i64::MAX);
        for bound in [&lo, &hi] {
            if let Const::Value(value) = bound
                && !i64_range.contains(value)
            {
                self.diag(
                    sink,
                    range,
                    DiagnosticCode::TypeBadIntBounds,
                    format!("the bound `{value}` of `Int` does not fit in `i64`"),
                );
                return Ty::error();
            }
        }
        if let (Const::Value(lo), Const::Value(hi)) = (&lo, &hi)
            && lo > hi
        {
            self.diag(
                sink,
                range,
                DiagnosticCode::TypeBadIntBounds,
                format!(
                    "`Int<{lo}, {hi}>` is empty: the lower bound is greater than the upper bound"
                ),
            );
            return Ty::error();
        }
        Ty::new(TyKind::Bounded { lo, hi })
    }

    /// The generic arguments for `def`'s own parameters; missing or extra arguments are
    /// reported.
    pub(crate) fn lower_args(
        &mut self,
        scope: Scope<'_>,
        def: ItemId,
        args: Option<&GenericArgs>,
        range: Anchor,
        sink: &mut Sink<'_>,
    ) -> Vec<GenericArg> {
        let params: Vec<bool> = match item_signature(self.db, self.program, def) {
            Some((signature, _)) => signature
                .generics
                .iter()
                .map(|param| matches!(param.kind, GenericParamKind::Const { .. }))
                .collect(),
            None => Vec::new(),
        };
        let given: &[HirGenericArg] = args.map_or(&[], |args| args.0.as_slice());
        if args.is_none() && sink.infer_args && sink.hole.is_some() {
            return params
                .iter()
                .map(|&is_const| {
                    sink.hole_arg(is_const)
                        .unwrap_or(GenericArg::Type(Ty::error()))
                })
                .collect();
        }
        if given.len() != params.len() {
            let name = item_name(self.db, def);
            let message = if args.is_none() {
                format!(
                    "missing generic arguments for `{name}`: it takes {}",
                    plural(params.len(), "generic argument")
                )
            } else {
                format!(
                    "`{name}` takes {} but {} supplied",
                    plural(params.len(), "generic argument"),
                    were(given.len())
                )
            };
            self.diag(sink, range, DiagnosticCode::TypeWrongGenericCount, message);
        }
        params
            .iter()
            .enumerate()
            .map(|(index, &is_const)| match given.get(index) {
                Some(arg) if is_const => GenericArg::Const(self.lower_const_arg(scope, arg, sink)),
                Some(arg) => GenericArg::Type(self.lower_type_arg(scope, arg, sink)),
                None if is_const => GenericArg::Const(Const::Error),
                None => GenericArg::Type(Ty::error()),
            })
            .collect()
    }

    pub(crate) fn lower_type_arg(
        &mut self,
        scope: Scope<'_>,
        arg: &HirGenericArg,
        sink: &mut Sink<'_>,
    ) -> Ty {
        match *arg {
            HirGenericArg::Type(ty) if !scope.res.const_args.contains_key(&ty) => {
                self.lower(scope, ty, sink)
            }
            HirGenericArg::Type(ty) => {
                let range = scope.node(NodeId::Type(ty));
                self.diag(
                    sink,
                    range,
                    DiagnosticCode::TypeExpectedType,
                    "expected a type, found a constant",
                );
                Ty::error()
            }
            HirGenericArg::Const(expr) => {
                let range = scope.node(NodeId::Expr(expr));
                self.diag(
                    sink,
                    range,
                    DiagnosticCode::TypeExpectedType,
                    "expected a type, found a constant",
                );
                Ty::error()
            }
        }
    }

    pub(crate) fn lower_const_arg(
        &mut self,
        scope: Scope<'_>,
        arg: &HirGenericArg,
        sink: &mut Sink<'_>,
    ) -> Const {
        match *arg {
            HirGenericArg::Const(expr) => self.fold(scope, expr, sink),
            HirGenericArg::Type(ty) => {
                let range = scope.node(NodeId::Type(ty));
                match scope.res.const_args.get(&ty) {
                    Some(Resolution::GenericParam { owner, index }) => Const::Param {
                        owner: *owner,
                        index: *index,
                    },
                    Some(Resolution::Def(Def::Item(item)) | Resolution::Assoc(item)) => {
                        let mut diags = Vec::new();
                        let value =
                            fold_const_item(self.db, self.program, *item, range, &mut diags);
                        if !sink.quiet {
                            sink.diags.extend(diags);
                        }
                        value
                    }
                    Some(Resolution::Error) => Const::Error,
                    Some(_) => {
                        self.diag(
                            sink,
                            range,
                            DiagnosticCode::TypeUnsupportedConst,
                            "only literals, arithmetic, constants and const parameters are allowed here",
                        );
                        Const::Error
                    }
                    None if matches!(scope.store.type_ref(ty), TypeRef::Infer)
                        && sink.hole.is_some() =>
                    {
                        match sink.hole_arg(true) {
                            Some(GenericArg::Const(c)) => c,
                            _ => Const::Error,
                        }
                    }
                    None => {
                        if !matches!(scope.res.types.get(&ty), Some(Resolution::Error)) {
                            self.diag(
                                sink,
                                range,
                                DiagnosticCode::TypeExpectedConst,
                                "expected a constant, found a type",
                            );
                        }
                        Const::Error
                    }
                }
            }
        }
    }

    fn self_type(&mut self, owner: ItemId) -> Ty {
        match item_tree(self.db, owner.file)
            .item(owner.index)
            .map(|data| &data.kind)
        {
            Some(ItemKind::Trait { .. }) => Ty::new(TyKind::TraitSelf { trait_item: owner }),
            Some(ItemKind::Impl { .. }) => self.impl_self_ty(owner),
            _ => Ty::error(),
        }
    }

    /// The self type of an impl, with the impl's own generic parameters.
    pub(crate) fn impl_self_ty(&mut self, impl_item: ItemId) -> Ty {
        if let Some(ty) = self.impl_selfs.get(&impl_item) {
            return ty.clone();
        }
        if self.impl_stack.contains(&impl_item) {
            return Ty::error();
        }
        let Some((signature, res)) = item_signature(self.db, self.program, impl_item) else {
            return Ty::error();
        };
        let SignatureKind::Impl { self_ty, .. } = &signature.kind else {
            return Ty::error();
        };
        let scope = Scope {
            src: Owner::Signature(impl_item),
            store: &signature.store,
            res,
        };
        self.impl_stack.push(impl_item);
        let ty = self.lower(scope, *self_ty, &mut Sink::quiet());
        self.impl_stack.pop();
        self.impl_selfs.insert(impl_item, ty.clone());
        ty
    }

    /// The value of a type alias with its own generic parameters; `None` for an associated
    /// type declared in a trait.
    pub(crate) fn alias_value(&mut self, alias: ItemId, range: Anchor) -> Option<Ty> {
        if let Some(value) = self.alias_values.get(&alias) {
            return value.clone();
        }
        if self.alias_stack.contains(&alias) {
            let name = item_name(self.db, alias);
            self.cycle_diags.push(Diag::new(
                range,
                DiagnosticCode::TypeAliasCycle,
                format!("cycle detected when expanding type alias `{name}`"),
            ));
            return Some(Ty::error());
        }
        let (signature, res) = item_signature(self.db, self.program, alias)?;
        let SignatureKind::TypeAlias { value, .. } = &signature.kind else {
            return Some(Ty::error());
        };
        let value = value.map(|value| {
            let scope = Scope {
                src: Owner::Signature(alias),
                store: &signature.store,
                res,
            };
            self.alias_stack.push(alias);
            let ty = self.lower(scope, value, &mut Sink::quiet());
            self.alias_stack.pop();
            ty
        });
        self.alias_values.insert(alias, value.clone());
        value
    }

    /// Lowers the value of `alias` reporting its diagnostics (the alias's own signature).
    pub(crate) fn lower_alias_item(
        &mut self,
        alias: ItemId,
        scope: Scope<'_>,
        value: TypeRefId,
        sink: &mut Sink<'_>,
    ) -> Ty {
        self.alias_stack.push(alias);
        let ty = self.lower(scope, value, sink);
        self.alias_stack.pop();
        ty
    }

    fn expand_alias(
        &mut self,
        alias: ItemId,
        args: &[GenericArg],
        range: Anchor,
        sink: &mut Sink<'_>,
    ) -> Ty {
        match self.alias_value(alias, range) {
            Some(value) => subst_ty(
                &value,
                &Subst {
                    owners: &[(alias, 0)],
                    args,
                    self_ty: None,
                },
            ),
            None => {
                let name = item_name(self.db, alias);
                self.diag(
                    sink,
                    range,
                    DiagnosticCode::TypeExpectedType,
                    format!("associated type `{name}` has no value here"),
                );
                Ty::error()
            }
        }
    }

    fn assoc_type(&mut self, item: ItemId, range: Anchor, sink: &mut Sink<'_>) -> Ty {
        let is_alias = matches!(
            item_tree(self.db, item.file)
                .item(item.index)
                .map(|data| &data.kind),
            Some(ItemKind::TypeAlias { .. })
        );
        if !is_alias {
            let name = item_name(self.db, item);
            self.diag(
                sink,
                range,
                DiagnosticCode::TypeExpectedType,
                format!("expected a type, found `{name}`"),
            );
            return Ty::error();
        }
        self.expand_alias(item, &[], range, sink)
    }

    fn deferred_type(
        &mut self,
        base: &Resolution,
        rest: &[Name],
        range: Anchor,
        sink: &mut Sink<'_>,
    ) -> Ty {
        let [name] = rest else {
            self.diag(
                sink,
                range,
                DiagnosticCode::TypeExpectedType,
                "ambiguous associated type",
            );
            return Ty::error();
        };
        let (base_ty, traits) = match base {
            Resolution::GenericParam { owner, index } => (
                Ty::new(TyKind::Param {
                    owner: *owner,
                    index: *index,
                }),
                self.param_bounds(*owner, *index),
            ),
            Resolution::SelfType(owner) => (
                Ty::new(TyKind::TraitSelf { trait_item: *owner }),
                vec![*owner],
            ),
            Resolution::Error => return Ty::error(),
            _ => {
                self.diag(
                    sink,
                    range,
                    DiagnosticCode::TypeExpectedType,
                    "ambiguous associated type",
                );
                return Ty::error();
            }
        };
        let found = traits.into_iter().find(|&trait_item| {
            trait_assoc(self.db, trait_item, name)
                .is_some_and(|assoc| assoc_is_type(self.db, assoc))
        });
        match found {
            Some(trait_item) => Ty::new(TyKind::Projection {
                base: base_ty,
                trait_item,
                name: name.clone(),
            }),
            None => {
                self.diag(
                    sink,
                    range,
                    DiagnosticCode::TypeExpectedType,
                    format!(
                        "associated type `{name}` not found: no bound of the parameter declares it"
                    ),
                );
                Ty::error()
            }
        }
    }

    /// The traits bounding a type parameter, as written.
    pub(crate) fn param_bounds(&self, owner: ItemId, index: u32) -> Vec<ItemId> {
        let Some((signature, res)) = item_signature(self.db, self.program, owner) else {
            return Vec::new();
        };
        let Some(param) = signature.generics.get(index as usize) else {
            return Vec::new();
        };
        let GenericParamKind::Type { bounds } = &param.kind else {
            return Vec::new();
        };
        bounds
            .iter()
            .filter_map(|bound| match res.types.get(bound) {
                Some(Resolution::Def(Def::Item(item))) => Some(*item),
                _ => None,
            })
            .collect()
    }
}

/// The item of `trait_item` named `name`.
pub(crate) fn trait_assoc(db: &dyn Db, trait_item: ItemId, name: &str) -> Option<ItemId> {
    let tree = item_tree(db, trait_item.file);
    let ItemKind::Trait { items } = &tree.item(trait_item.index)?.kind else {
        return None;
    };
    items
        .iter()
        .copied()
        .find(|&index| tree.item(index).and_then(|data| data.name.as_deref()) == Some(name))
        .map(|index| ItemId {
            file: trait_item.file,
            index,
        })
}

fn assoc_is_type(db: &dyn Db, item: ItemId) -> bool {
    matches!(
        item_tree(db, item.file)
            .item(item.index)
            .map(|data| &data.kind),
        Some(ItemKind::TypeAlias { .. })
    )
}

pub(crate) fn plural(count: usize, what: &str) -> String {
    if count == 1 {
        format!("1 {what}")
    } else {
        format!("{count} {what}s")
    }
}

pub(crate) fn were(count: usize) -> String {
    if count == 1 {
        "1 was".to_owned()
    } else {
        format!("{count} were")
    }
}
