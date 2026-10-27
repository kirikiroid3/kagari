//! Value paths, struct expressions and the type named by a path prefix.

use super::ValueRef;
use super::ctx::{Expect, InferCtx};
use crate::def_map::{Def, Namespace, Resolution, resolve_module_path};
use crate::diagnostic::{Anchor, DiagnosticCode, NodeId, Owner};
use crate::hir::{ExprId, GenericArgs, Name, Path, PathKind};
use crate::item_tree::{ItemId, ItemKind, Shape, item_tree};
use crate::signatures::lower::{item_name, plural, trait_assoc, were};
use crate::signatures::subst::{Subst, subst_ty};
use crate::signatures::{GenericParamDefKind, ItemSig, assoc_parent};
use crate::ty::{CtorRef, FnDefId, FnRef, GenericArg, Ty, TyKind};

impl InferCtx<'_> {
    pub(crate) fn infer_path(&mut self, expr: ExprId, path: &Path) -> Ty {
        self.infer_path_arg_exprs(path);
        let Some(resolution) = self.res.store.exprs.get(&expr).cloned() else {
            return Ty::error();
        };
        let range = self.expr_anchor(expr);
        let (value, ty) = self.path_value(expr, path, &resolution, range);
        self.result.paths.insert(expr, value);
        ty
    }

    fn path_value(
        &mut self,
        expr: ExprId,
        path: &Path,
        resolution: &Resolution,
        range: Anchor,
    ) -> (ValueRef, Ty) {
        let _ = expr;
        let last_args = path
            .segments
            .last()
            .and_then(|segment| segment.args.as_ref());
        match resolution {
            Resolution::Local(local) => (ValueRef::Local(*local), self.local_ty(*local)),
            Resolution::Def(Def::Item(item)) => self.item_value(*item, last_args, None, range),
            Resolution::Def(Def::Variant { enum_item, index }) => {
                let args = self.adt_args_from_path(path, *enum_item, range);
                self.variant_value(*enum_item, Some(*index), args, range)
            }
            Resolution::GenericParam { owner, index } => {
                let ty = self
                    .sigs
                    .generics(*owner)
                    .and_then(|generics| generics.params.get(*index as usize))
                    .map(|param| match &param.kind {
                        GenericParamDefKind::Const { ty } => Some(ty.clone()),
                        GenericParamDefKind::Type { .. } => None,
                    });
                match ty {
                    Some(Some(ty)) => (
                        ValueRef::ConstParam {
                            owner: *owner,
                            index: *index,
                        },
                        ty,
                    ),
                    _ => self.not_a_value(range, "a type parameter"),
                }
            }
            Resolution::Assoc(item) => self.assoc_value(expr, path, *item, range),
            Resolution::SelfType(owner) => {
                let self_ty = self.lowerer.impl_self_ty(*owner);
                match self_ty.kind() {
                    TyKind::Adt { item, args }
                        if self.sigs.adt(*item).is_some_and(|adt| !adt.is_enum) =>
                    {
                        self.variant_value(*item, None, args.to_vec(), range)
                    }
                    _ => self.not_a_value(range, "`Self`"),
                }
            }
            Resolution::Deferred { base, rest } => {
                self.deferred_value(base, rest, last_args, range)
            }
            Resolution::Prim(prim) => {
                self.not_a_value(range, &format!("the type `{}`", prim.name()))
            }
            Resolution::Def(Def::Module(_)) => self.not_a_value(range, "a module"),
            Resolution::Error => (ValueRef::Error, Ty::error()),
        }
    }

    fn not_a_value(&mut self, range: Anchor, what: &str) -> (ValueRef, Ty) {
        self.diag(
            range,
            DiagnosticCode::TypeExpectedValue,
            format!("expected a value, found {what}"),
        );
        (ValueRef::Error, Ty::error())
    }

    /// A path to an item used as a value. `parent_args` are the generic arguments of the
    /// item's impl, when the path reaches it through one.
    fn item_value(
        &mut self,
        item: ItemId,
        last_args: Option<&GenericArgs>,
        parent_args: Option<Vec<GenericArg>>,
        range: Anchor,
    ) -> (ValueRef, Ty) {
        let tree = item_tree(self.db, item.file);
        let Some(data) = tree.item(item.index) else {
            return (ValueRef::Error, Ty::error());
        };
        match &data.kind {
            ItemKind::Fn { .. } => {
                let fn_ref = self.fn_ref(item, None, parent_args, last_args, range);
                self.add_fn_obligations(&fn_ref, range);
                let ty = Ty::new(TyKind::FnDef(fn_ref.clone()));
                (ValueRef::Fn(fn_ref), ty)
            }
            ItemKind::Struct { .. } => {
                let args = self.adt_args(item, last_args, range);
                self.variant_value(item, None, args, range)
            }
            ItemKind::Const { .. } => {
                let args = parent_args.unwrap_or_default();
                let ty = match self.sigs.get(item) {
                    Some(ItemSig::Const { ty, .. }) => {
                        let owners = self.owners_of(item);
                        let ty = subst_ty(
                            ty,
                            &Subst {
                                owners: &owners,
                                args: &args,
                                self_ty: None,
                            },
                        );
                        self.normalize(&ty)
                    }
                    _ => Ty::error(),
                };
                (ValueRef::Const { item, args }, ty)
            }
            ItemKind::Static { .. } => {
                let ty = match self.sigs.get(item) {
                    Some(ItemSig::Static { ty, .. }) => ty.clone(),
                    _ => Ty::error(),
                };
                (ValueRef::Static(item), ty)
            }
            kind => {
                let what = match kind {
                    ItemKind::Enum { .. } => "enum",
                    ItemKind::TypeAlias { .. } => "type alias",
                    ItemKind::Trait { .. } => "trait",
                    _ => data.kind_name(),
                };
                let name = data.name.clone().unwrap_or_default();
                self.not_a_value(range, &format!("{what} `{name}`"))
            }
        }
    }

    /// A struct (`variant: None`) or enum variant used as a value: a tuple constructor or a
    /// unit value.
    fn variant_value(
        &mut self,
        adt: ItemId,
        variant: Option<u32>,
        args: Vec<GenericArg>,
        range: Anchor,
    ) -> (ValueRef, Ty) {
        let Some(def) = self.sigs.adt(adt) else {
            return (ValueRef::Error, Ty::error());
        };
        let Some(variant_def) = def.variants.get(variant.unwrap_or(0) as usize) else {
            return (ValueRef::Error, Ty::error());
        };
        match &variant_def.shape {
            Shape::Tuple(_) => {
                let def_id = match variant {
                    Some(index) => FnDefId::VariantCtor(adt, index),
                    None => FnDefId::StructCtor(adt),
                };
                let fn_ref = FnRef {
                    def: def_id,
                    self_ty: None,
                    args: crate::ty::GenericArgs::new(args),
                };
                let ty = Ty::new(TyKind::FnDef(fn_ref.clone()));
                (ValueRef::Fn(fn_ref), ty)
            }
            Shape::Unit => {
                let args = crate::ty::GenericArgs::new(args);
                let ty = Ty::adt(adt, args.clone());
                (ValueRef::Ctor(CtorRef { adt, variant, args }), ty)
            }
            Shape::Record(_) => {
                let name = variant_def.name.clone();
                let what = if variant.is_some() {
                    "struct variant"
                } else {
                    "struct"
                };
                self.diag(
                    range,
                    DiagnosticCode::TypeExpectedValue,
                    format!("expected a value, found {what} `{name}`; write `{name} {{ … }}`"),
                );
                (ValueRef::Error, Ty::error())
            }
        }
    }

    /// The generic arguments of a function: the parent's (given or fresh) followed by its own
    /// (from `last_args` or fresh).
    pub(crate) fn fn_ref(
        &mut self,
        item: ItemId,
        self_ty: Option<Ty>,
        parent_args: Option<Vec<GenericArg>>,
        own_args: Option<&GenericArgs>,
        range: Anchor,
    ) -> FnRef {
        let mut args = match parent_args {
            Some(args) => args,
            None => match self
                .sigs
                .generics(item)
                .and_then(|generics| generics.parent)
            {
                Some(parent) => self.fresh_args_of(parent),
                None => Vec::new(),
            },
        };
        args.extend(self.own_args(item, own_args, range));
        FnRef {
            def: FnDefId::Item(item),
            self_ty,
            args: crate::ty::GenericArgs::new(args),
        }
    }

    /// The arguments for `item`'s own generic parameters: a turbofish or fresh variables.
    pub(crate) fn own_args(
        &mut self,
        item: ItemId,
        given: Option<&GenericArgs>,
        range: Anchor,
    ) -> Vec<GenericArg> {
        let Some(generics) = self.sigs.generics(item) else {
            return Vec::new();
        };
        let kinds: Vec<bool> = generics
            .params
            .iter()
            .map(|param| matches!(param.kind, GenericParamDefKind::Const { .. }))
            .collect();
        let Some(given) = given else {
            return self.fresh_args(generics);
        };
        if given.0.len() != kinds.len() {
            let name = item_name(self.db, item);
            self.diag(
                range,
                DiagnosticCode::TypeWrongGenericCount,
                format!(
                    "`{name}` takes {} but {} supplied",
                    plural(kinds.len(), "generic argument"),
                    were(given.0.len())
                ),
            );
        }
        kinds
            .iter()
            .enumerate()
            .map(|(index, &is_const)| match given.0.get(index) {
                Some(arg) if is_const => {
                    GenericArg::Const(self.with_expr_sink(|lowerer, scope, sink| {
                        lowerer.lower_const_arg(scope, arg, sink)
                    }))
                }
                Some(arg) => GenericArg::Type(self.with_expr_sink(|lowerer, scope, sink| {
                    lowerer.lower_type_arg(scope, arg, sink)
                })),
                None if is_const => GenericArg::Const(self.table.new_const()),
                None => GenericArg::Type(self.table.new_var()),
            })
            .collect()
    }

    /// The arguments of an ADT written as a path segment (`Vec::<u32>`) or fresh ones.
    fn adt_args(
        &mut self,
        adt: ItemId,
        given: Option<&GenericArgs>,
        range: Anchor,
    ) -> Vec<GenericArg> {
        self.own_args(adt, given, range)
    }

    /// The arguments of the enum of a variant path: `Option::<u32>::None` or `None::<u32>`.
    fn adt_args_from_path(&mut self, path: &Path, adt: ItemId, range: Anchor) -> Vec<GenericArg> {
        let len = path.segments.len();
        let given = len
            .checked_sub(2)
            .and_then(|index| path.segments.get(index))
            .and_then(|segment| segment.args.as_ref())
            .or_else(|| {
                path.segments
                    .last()
                    .and_then(|segment| segment.args.as_ref())
            });
        self.adt_args(adt, given, range)
    }

    /// An associated function or constant reached through a type or trait path.
    fn assoc_value(
        &mut self,
        expr: ExprId,
        path: &Path,
        item: ItemId,
        range: Anchor,
    ) -> (ValueRef, Ty) {
        let Some(parent) = assoc_parent(self.db, item) else {
            return (ValueRef::Error, Ty::error());
        };
        let last_args = path
            .segments
            .last()
            .and_then(|segment| segment.args.as_ref());
        match self.sigs.get(parent) {
            Some(ItemSig::Trait { .. }) => {
                let is_fn = matches!(
                    item_tree(self.db, item.file)
                        .item(item.index)
                        .map(|data| &data.kind),
                    Some(ItemKind::Fn { .. })
                );
                if !is_fn {
                    return self.not_a_value(range, "an associated item of a trait");
                }
                let self_ty = self.table.new_var();
                let parent_args = self.fresh_args_of(parent);
                let fn_ref = self.fn_ref(item, Some(self_ty), Some(parent_args), last_args, range);
                self.add_fn_obligations(&fn_ref, range);
                let ty = Ty::new(TyKind::FnDef(fn_ref.clone()));
                (ValueRef::Fn(fn_ref), ty)
            }
            Some(ItemSig::Impl { .. }) => {
                let impl_args = self.impl_args_from_prefix(expr, path, parent);
                self.item_value(item, last_args, Some(impl_args), range)
            }
            _ => (ValueRef::Error, Ty::error()),
        }
    }

    /// Fresh arguments for `impl_item`'s parameters, fixed by the type the path names before
    /// its last segment (`Board::new` with `type Board = Square<8>`).
    pub(crate) fn impl_args_from_prefix(
        &mut self,
        expr: ExprId,
        path: &Path,
        impl_item: ItemId,
    ) -> Vec<GenericArg> {
        let args = self.fresh_args_of(impl_item);
        if let Some(prefix) = self.prefix_type(expr, path) {
            let self_ty = match self.sigs.get(impl_item) {
                Some(ItemSig::Impl { self_ty, .. }) => subst_ty(
                    self_ty,
                    &Subst {
                        owners: &[(impl_item, 0)],
                        args: &args,
                        self_ty: None,
                    },
                ),
                _ => Ty::error(),
            };
            self.try_unify(&self_ty, &prefix);
        }
        args
    }

    /// The type named by all segments of `path` (the path of `expr`) but the last, if it
    /// names one.
    fn prefix_type(&mut self, expr: ExprId, path: &Path) -> Option<Ty> {
        let len = path.segments.len().checked_sub(1)?;
        if len == 0 {
            return None;
        }
        let prefix = Path {
            kind: path.kind,
            segments: path.segments.get(..len)?.to_vec(),
        };
        let resolution = self.resolve_type_path(expr, &prefix)?;
        let args = prefix
            .segments
            .last()
            .and_then(|segment| segment.args.clone());
        let range = Anchor::Segment(
            Owner::Body(self.item),
            NodeId::Expr(expr),
            u32::try_from(len.checked_sub(1)?).ok()?,
        );
        let ty = self.with_expr_sink(|lowerer, scope, sink| {
            sink.quiet = false;
            lowerer.lower_resolution(scope, &resolution, args.as_ref(), range, sink)
        });
        Some(ty)
    }

    /// Resolves a type path (a prefix of the path of `expr`) with the body's lexical scope:
    /// `Self`, generic parameters, then modules.
    fn resolve_type_path(&self, expr: ExprId, path: &Path) -> Option<Resolution> {
        if path.kind == PathKind::Plain
            && let [segment] = path.segments.as_slice()
        {
            if segment.name == "Self" {
                return self.self_owner.map(Resolution::SelfType);
            }
            if let Some((_, owner, index, _)) = self
                .generic_scope
                .iter()
                .rev()
                .find(|(name, _, _, is_const)| *name == segment.name && !is_const)
            {
                return Some(Resolution::GenericParam {
                    owner: *owner,
                    index: *index,
                });
            }
        }
        let module = self.current_module();
        let owner = Owner::Body(self.item);
        let at = |segment| Anchor::Segment(owner, NodeId::Expr(expr), segment);
        match resolve_module_path(self.maps, self.db, module, path, Namespace::Types, &at).resolved
        {
            Some(Resolution::Def(Def::Item(item)))
                if matches!(
                    item_tree(self.db, item.file)
                        .item(item.index)
                        .map(|data| &data.kind),
                    Some(ItemKind::Trait { .. })
                ) =>
            {
                None
            }
            other => other,
        }
    }

    /// `T::f` for a type parameter `T` (or `Self` in a trait): a method of one of its bounds.
    fn deferred_value(
        &mut self,
        base: &Resolution,
        rest: &[Name],
        last_args: Option<&GenericArgs>,
        range: Anchor,
    ) -> (ValueRef, Ty) {
        let (base_ty, traits) = match base {
            Resolution::GenericParam { owner, index } => (
                Ty::new(TyKind::Param {
                    owner: *owner,
                    index: *index,
                }),
                self.env.bounds_of(*owner, *index).to_vec(),
            ),
            Resolution::SelfType(owner) => (
                Ty::new(TyKind::TraitSelf { trait_item: *owner }),
                vec![*owner],
            ),
            Resolution::Error => return (ValueRef::Error, Ty::error()),
            _ => return self.not_a_value(range, "an associated item of an unknown type"),
        };
        let [name] = rest else {
            return self.not_a_value(range, "an ambiguous associated item");
        };
        let found = traits.iter().find_map(|&trait_item| {
            let assoc = trait_assoc(self.db, trait_item, name)?;
            matches!(
                item_tree(self.db, assoc.file)
                    .item(assoc.index)
                    .map(|data| &data.kind),
                Some(ItemKind::Fn { .. })
            )
            .then_some((trait_item, assoc))
        });
        let Some((trait_item, assoc)) = found else {
            let message = format!(
                "no function `{name}` in the bounds of `{}`",
                self.show(&base_ty)
            );
            self.diag(range, DiagnosticCode::TypeUnknownMethod, message);
            return (ValueRef::Error, Ty::error());
        };
        let parent_args = self.fresh_args_of(trait_item);
        let fn_ref = self.fn_ref(assoc, Some(base_ty), Some(parent_args), last_args, range);
        self.add_fn_obligations(&fn_ref, range);
        let ty = Ty::new(TyKind::FnDef(fn_ref.clone()));
        (ValueRef::Fn(fn_ref), ty)
    }

    /// The struct or variant a struct expression or pattern names, with its generic arguments.
    pub(crate) fn ctor_of(
        &mut self,
        resolution: Option<&Resolution>,
        path: &Path,
        range: Anchor,
    ) -> Option<CtorRef> {
        match resolution? {
            Resolution::Def(Def::Item(item)) => {
                match item_tree(self.db, item.file)
                    .item(item.index)
                    .map(|data| &data.kind)
                {
                    Some(ItemKind::Struct { .. }) => {
                        let given = path
                            .segments
                            .last()
                            .and_then(|segment| segment.args.as_ref());
                        let args = self.adt_args(*item, given, range);
                        Some(CtorRef {
                            adt: *item,
                            variant: None,
                            args: crate::ty::GenericArgs::new(args),
                        })
                    }
                    Some(ItemKind::TypeAlias { .. }) => {
                        let resolution = Resolution::Def(Def::Item(*item));
                        let given = path
                            .segments
                            .last()
                            .and_then(|segment| segment.args.clone());
                        let ty = self.with_expr_sink(|lowerer, scope, sink| {
                            lowerer.lower_resolution(
                                scope,
                                &resolution,
                                given.as_ref(),
                                range,
                                sink,
                            )
                        });
                        self.struct_ctor_of_ty(ty, range)
                    }
                    _ => {
                        let name = item_name(self.db, *item);
                        self.diag(
                            range,
                            DiagnosticCode::TypeExpectedType,
                            format!("expected a struct or variant, found `{name}`"),
                        );
                        None
                    }
                }
            }
            Resolution::Def(Def::Variant { enum_item, index }) => {
                let args = self.adt_args_from_path(path, *enum_item, range);
                Some(CtorRef {
                    adt: *enum_item,
                    variant: Some(*index),
                    args: crate::ty::GenericArgs::new(args),
                })
            }
            Resolution::SelfType(owner) => {
                let ty = self.lowerer.impl_self_ty(*owner);
                self.struct_ctor_of_ty(ty, range)
            }
            Resolution::Error => None,
            _ => {
                self.diag(
                    range,
                    DiagnosticCode::TypeExpectedType,
                    "expected a struct or variant",
                );
                None
            }
        }
    }

    fn struct_ctor_of_ty(&mut self, ty: Ty, range: Anchor) -> Option<CtorRef> {
        match ty.kind() {
            TyKind::Adt { item, args } if self.sigs.adt(*item).is_some_and(|adt| !adt.is_enum) => {
                Some(CtorRef {
                    adt: *item,
                    variant: None,
                    args: args.clone(),
                })
            }
            TyKind::Error => None,
            _ => {
                let other = ty.clone();
                let message = format!("expected a struct, found `{}`", self.show(&other));
                self.diag(range, DiagnosticCode::TypeExpectedType, message);
                None
            }
        }
    }

    pub(crate) fn infer_record(
        &mut self,
        expr: ExprId,
        path: &Path,
        fields: &[(Name, ExprId)],
        expect: Expect<'_>,
    ) -> Ty {
        let range = self.expr_anchor(expr);
        let resolution = self.res.store.exprs.get(&expr).cloned();
        let Some(ctor) = self.ctor_of(resolution.as_ref(), path, range) else {
            for (_, value) in fields {
                self.infer_expr(*value, Expect::None);
            }
            return Ty::error();
        };
        let ty = Ty::adt(ctor.adt, ctor.args.clone());
        if let Some(expected) = expect.ty() {
            self.try_unify(&ty, expected);
        }
        let Some(def) = self.sigs.adt(ctor.adt) else {
            return Ty::error();
        };
        if def.is_enum && ctor.variant.is_none() {
            let name = item_name(self.db, ctor.adt);
            self.diag(
                range,
                DiagnosticCode::TypeExpectedType,
                format!("expected a struct or variant, found enum `{name}`"),
            );
            for (_, value) in fields {
                self.infer_expr(*value, Expect::None);
            }
            return Ty::error();
        }
        let Some(variant) = def.variants.get(ctor.variant.unwrap_or(0) as usize) else {
            return Ty::error();
        };
        let variant_name = variant.name.clone();
        let subst = Subst {
            owners: &[(ctor.adt, 0)],
            args: &ctor.args,
            self_ty: None,
        };
        let field_defs: Vec<(Option<Name>, Ty)> = variant
            .fields
            .iter()
            .map(|field| (field.name.clone(), subst_ty(&field.ty, &subst)))
            .collect();
        let mut seen: Vec<Name> = Vec::new();
        for (name, value) in fields {
            match field_defs
                .iter()
                .find(|(field, _)| field.as_ref() == Some(name))
            {
                Some((_, field_ty)) => {
                    let field_ty = self.normalize(field_ty);
                    self.infer_expr(*value, Expect::Coerce(&field_ty));
                    seen.push(name.clone());
                }
                None => {
                    let target = self.expr_anchor(*value);
                    self.diag(
                        target,
                        DiagnosticCode::TypeUnknownField,
                        format!("`{variant_name}` has no field named `{name}`"),
                    );
                    self.infer_expr(*value, Expect::None);
                }
            }
        }
        let missing: Vec<String> = field_defs
            .iter()
            .filter_map(|(name, _)| name.as_ref())
            .filter(|name| !seen.contains(name))
            .map(|name| format!("`{name}`"))
            .collect();
        if !missing.is_empty() {
            let what = if missing.len() == 1 {
                "field"
            } else {
                "fields"
            };
            self.diag(
                range,
                DiagnosticCode::TypeMissingField,
                format!(
                    "missing {what} {} in initializer of `{variant_name}`",
                    missing.join(", ")
                ),
            );
        }
        self.result.records.insert(expr, ctor);
        ty
    }
}
