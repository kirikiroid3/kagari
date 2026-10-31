//! Trait implementation: explicit std impls, bounds of generic parameters, and the structural
//! `Eq` rule.

use rustc_hash::{FxHashMap, FxHashSet};

use super::subst::{Subst, match_ty, subst_ty};
use super::{GenericParamDefKind, ItemSig, ProgramSignatures, program_signatures};
use crate::db::Db;
use crate::def_map::{DefMaps, PrimType, TypeKey, def_maps};
use crate::hir::Name;
use crate::input::Program;
use crate::item_tree::{ItemId, ItemKind, item_tree};
use crate::ty::{GenericArg, IntTy, Ty, TyKind};

/// The trait bounds of the generic parameters in scope: `(owner, index)` → traits.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoundEnv {
    pub bounds: FxHashMap<(ItemId, u32), Vec<ItemId>>,
}

impl BoundEnv {
    /// The bounds of `item`'s own generic parameters and of its parent impl's or trait's.
    pub fn of_item(sigs: &ProgramSignatures, item: ItemId) -> BoundEnv {
        let mut env = BoundEnv::default();
        if let Some(generics) = sigs.generics(item) {
            if let Some(parent) = generics.parent {
                env.add(sigs, parent);
            }
            env.add(sigs, item);
        }
        env
    }

    fn add(&mut self, sigs: &ProgramSignatures, owner: ItemId) {
        let Some(generics) = sigs.generics(owner) else {
            return;
        };
        for (index, param) in generics.params.iter().enumerate() {
            if let GenericParamDefKind::Type { bounds } = &param.kind {
                let index = u32::try_from(index).unwrap_or(u32::MAX);
                self.bounds.insert((owner, index), bounds.clone());
            }
        }
    }

    pub fn bounds_of(&self, owner: ItemId, index: u32) -> &[ItemId] {
        self.bounds.get(&(owner, index)).map_or(&[], Vec::as_slice)
    }
}

/// Whether `ty` implements `trait_item`: through an `impl` in std (matching the impl's
/// generics and checking their bounds), a bound in `env`, or for `Eq` structurally.
/// `Ty::Error` implements every trait.
pub fn implements(
    db: &dyn Db,
    program: Program,
    ty: &Ty,
    trait_item: ItemId,
    env: &BoundEnv,
) -> bool {
    let sigs = program_signatures(db, program);
    let maps = def_maps(db, program);
    TraitSolver {
        sigs,
        maps,
        env,
        visiting: FxHashSet::default(),
    }
    .implements(ty, trait_item)
}

pub(crate) fn type_key(ty: &Ty) -> Option<TypeKey> {
    Some(match ty.kind() {
        TyKind::Adt { item, .. } => TypeKey::Item(*item),
        TyKind::Bool => TypeKey::Prim(PrimType::Bool),
        TyKind::Bounded { .. } => TypeKey::Prim(PrimType::Int),
        TyKind::Int(int) => TypeKey::Prim(match int {
            IntTy::U8 => PrimType::U8,
            IntTy::U16 => PrimType::U16,
            IntTy::U32 => PrimType::U32,
            IntTy::U64 => PrimType::U64,
            IntTy::U128 => PrimType::U128,
            IntTy::Usize => PrimType::Usize,
            IntTy::I8 => PrimType::I8,
            IntTy::I16 => PrimType::I16,
            IntTy::I32 => PrimType::I32,
            IntTy::I64 => PrimType::I64,
            IntTy::I128 => PrimType::I128,
            IntTy::Isize => PrimType::Isize,
        }),
        _ => return None,
    })
}

/// The generic arguments of `impl_item` that make its self type `ty`, if it matches.
pub(crate) fn match_impl(
    sigs: &ProgramSignatures,
    impl_item: ItemId,
    ty: &Ty,
) -> Option<Vec<GenericArg>> {
    let Some(ItemSig::Impl {
        generics, self_ty, ..
    }) = sigs.get(impl_item)
    else {
        return None;
    };
    let mut out: Vec<Option<GenericArg>> = vec![None; generics.params.len()];
    if !match_ty(self_ty, ty, impl_item, &mut out) {
        return None;
    }
    Some(
        out.into_iter()
            .zip(&generics.params)
            .map(|(arg, param)| {
                arg.unwrap_or(match param.kind {
                    GenericParamDefKind::Type { .. } => GenericArg::Type(Ty::error()),
                    GenericParamDefKind::Const { .. } => GenericArg::Const(crate::ty::Const::Error),
                })
            })
            .collect(),
    )
}

/// The trait impls of `trait_item` whose self type can match `ty`.
pub(crate) fn trait_impls_for<'m>(
    maps: &'m DefMaps,
    ty: &Ty,
    trait_item: ItemId,
) -> impl Iterator<Item = ItemId> + 'm {
    type_key(ty)
        .and_then(|key| maps.trait_impls.get(&key))
        .into_iter()
        .flatten()
        .copied()
        .filter(move |impl_item| maps.impl_traits.get(impl_item) == Some(&trait_item))
}

/// Normalizes `<base as trait_item>::name` for a `base` that is not a parameter: the value of
/// the associated type in the matching std impl.
pub(crate) fn normalize_projection(
    db: &dyn Db,
    sigs: &ProgramSignatures,
    maps: &DefMaps,
    base: &Ty,
    trait_item: ItemId,
    name: &Name,
) -> Option<Ty> {
    for impl_item in trait_impls_for(maps, base, trait_item) {
        let Some(args) = match_impl(sigs, impl_item, base) else {
            continue;
        };
        let tree = item_tree(db, impl_item.file);
        let Some(ItemKind::Impl { items, .. }) = tree.item(impl_item.index).map(|data| &data.kind)
        else {
            continue;
        };
        let assoc = items
            .iter()
            .copied()
            .find(|&index| tree.item(index).and_then(|data| data.name.as_ref()) == Some(name));
        let Some(assoc) = assoc else {
            continue;
        };
        let assoc = ItemId {
            file: impl_item.file,
            index: assoc,
        };
        if let Some(ItemSig::TypeAlias {
            ty: Some(value), ..
        }) = sigs.get(assoc)
        {
            return Some(subst_ty(
                value,
                &Subst {
                    owners: &[(impl_item, 0)],
                    args: &args,
                    self_ty: None,
                },
            ));
        }
    }
    None
}

pub(crate) struct TraitSolver<'a> {
    pub sigs: &'a ProgramSignatures,
    pub maps: &'a DefMaps,
    pub env: &'a BoundEnv,
    pub visiting: FxHashSet<Ty>,
}

impl TraitSolver<'_> {
    pub(crate) fn implements(&mut self, ty: &Ty, trait_item: ItemId) -> bool {
        // Every integer is a `FromInteger`, including a parameter bounded by `Integer`.
        if Some(trait_item) == self.sigs.std.from_integer
            && let Some(integer) = self.sigs.std.integer
            && self.implements(ty, integer)
        {
            return true;
        }
        match ty.kind() {
            TyKind::Error | TyKind::Infer(_) | TyKind::Never => return true,
            TyKind::Param { owner, index } => {
                return self.env.bounds_of(*owner, *index).contains(&trait_item);
            }
            TyKind::TraitSelf { trait_item: own } => return *own == trait_item,
            _ => {}
        }
        if self.explicit_impl(ty, trait_item) {
            return true;
        }
        if Some(trait_item) == self.sigs.std.eq {
            return self.structural_eq(ty, trait_item);
        }
        if Some(trait_item) == self.sigs.std.rand_choose {
            // Fieldless enums; `Int<lo, hi>` has an explicit impl, checked above.
            return matches!(ty.kind(), TyKind::Adt { item, .. } if self.sigs.adt(*item).is_some_and(|adt| {
                adt.is_enum
                    && !adt.builtin
                    && !adt.variants.is_empty()
                    && adt.variants.iter().all(|variant| variant.fields.is_empty())
            }));
        }
        if Some(trait_item) == self.sigs.std.shader_arg {
            // `()` and `#[shader_arg]` structs.
            return match ty.kind() {
                TyKind::Tuple(elems) => elems.is_empty(),
                TyKind::Adt { item, .. } => self.sigs.adt(*item).is_some_and(|adt| adt.shader_arg),
                _ => false,
            };
        }
        false
    }

    fn explicit_impl(&mut self, ty: &Ty, trait_item: ItemId) -> bool {
        let impls: Vec<ItemId> = trait_impls_for(self.maps, ty, trait_item).collect();
        for impl_item in impls {
            let Some(args) = match_impl(self.sigs, impl_item, ty) else {
                continue;
            };
            let Some(generics) = self.sigs.generics(impl_item) else {
                continue;
            };
            let bounds_hold =
                generics
                    .params
                    .iter()
                    .zip(&args)
                    .all(|(param, arg)| match (&param.kind, arg) {
                        (GenericParamDefKind::Type { bounds }, GenericArg::Type(arg)) => {
                            let bounds = bounds.clone();
                            bounds.into_iter().all(|bound| self.implements(arg, bound))
                        }
                        _ => true,
                    });
            if bounds_hold {
                return true;
            }
        }
        false
    }

    fn structural_eq(&mut self, ty: &Ty, eq: ItemId) -> bool {
        match ty.kind() {
            TyKind::Bool
            | TyKind::Int(_)
            | TyKind::Bounded { .. }
            | TyKind::FnPtr { .. }
            | TyKind::FnDef(_) => true,
            TyKind::Closure { .. } | TyKind::Projection { .. } => false,
            TyKind::Tuple(fields) => fields.iter().all(|field| self.implements(field, eq)),
            TyKind::Array { elem, .. } => self.implements(elem, eq),
            TyKind::Ref { inner, .. } => self.implements(inner, eq),
            TyKind::Adt { item, args } => {
                let Some(adt) = self.sigs.adt(*item) else {
                    return true;
                };
                if adt.builtin {
                    return false;
                }
                if !self.visiting.insert(ty.clone()) {
                    return true;
                }
                let subst = Subst {
                    owners: &[(*item, 0)],
                    args,
                    self_ty: None,
                };
                let fields: Vec<Ty> = adt
                    .variants
                    .iter()
                    .flat_map(|variant| &variant.fields)
                    .map(|field| subst_ty(&field.ty, &subst))
                    .collect();
                let all = fields.iter().all(|field| self.implements(field, eq));
                self.visiting.remove(ty);
                all
            }
            TyKind::Error
            | TyKind::Never
            | TyKind::Infer(_)
            | TyKind::Param { .. }
            | TyKind::TraitSelf { .. } => true,
        }
    }
}
