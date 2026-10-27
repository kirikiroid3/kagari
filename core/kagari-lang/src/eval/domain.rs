//! The values of finite types (`docs/syntax/src/kinds.md#finite-types`), for tabulating
//! `const fn` calls from the story.

use rustc_hash::FxHashMap;

use crate::hir::FnPtrKind;
use crate::item_tree::{FnKind, ItemId};
use crate::signatures::{FnSig, ProgramSignatures};
use crate::ty::{FnDefId, FnRef, GenericArg, GenericArgs, Ty, TyKind};

use super::machine::value::{GenericEnv, subst_ty};
use super::std_items::StdTable;
use super::value::{AssetKind, ConstValue, Text};

/// Types nested deeper than this are treated as not finite (they can only be recursive).
const MAX_DEPTH: u32 = 32;

/// What the finite types built from program data range over.
pub struct DomainCtx<'a> {
    pub sigs: &'a ProgramSignatures,
    pub std: &'a StdTable,
    /// The constant texts of the program: the values of `Text`.
    pub texts: &'a [Text],
    /// The asset constants of the program by kind: the values of the asset types.
    pub assets: &'a FxHashMap<AssetKind, Vec<ConstValue>>,
    /// The named non-generic functions of the program with their signatures, in a fixed
    /// order: the values of `const fn`, `pushdown fn` and `scene fn` pointer types.
    pub functions: &'a [(ItemId, &'a FnSig)],
}

impl DomainCtx<'_> {
    /// The number of values of `ty` (saturating at `u64::MAX`), or `None` when `ty` is not
    /// finite.
    pub fn size(&self, ty: &Ty) -> Option<u64> {
        self.size_at(ty, 0)
    }

    fn size_at(&self, ty: &Ty, depth: u32) -> Option<u64> {
        if depth > MAX_DEPTH {
            return None;
        }
        let depth = depth.saturating_add(1);
        match ty.kind() {
            TyKind::Bool => Some(2),
            TyKind::Bounded { lo, hi } => {
                let (lo, hi) = (lo.value()?, hi.value()?);
                if hi < lo {
                    return Some(0);
                }
                Some(
                    u64::try_from(hi.checked_sub(lo)?)
                        .unwrap_or(u64::MAX)
                        .saturating_add(1),
                )
            }
            TyKind::Tuple(fields) => fields.iter().try_fold(1u64, |acc, field| {
                Some(acc.saturating_mul(self.size_at(field, depth)?))
            }),
            TyKind::Array { elem, len } => {
                let len = u32::try_from(len.value()?).ok()?;
                Some(self.size_at(elem, depth)?.saturating_pow(len))
            }
            TyKind::Ref { inner, .. } => self.size_at(inner, depth),
            TyKind::FnPtr { kind, params, ret } => Some(
                u64::try_from(self.functions_of(*kind, params, ret)?.len()).unwrap_or(u64::MAX),
            ),
            TyKind::Adt { item, args } => {
                if *item == self.std.text {
                    return Some(u64::try_from(self.texts.len()).unwrap_or(u64::MAX));
                }
                if let Some(kind) = self.std.asset_kind(*item) {
                    let count = self.assets.get(&kind).map_or(0, Vec::len);
                    return Some(u64::try_from(count).unwrap_or(u64::MAX));
                }
                let adt = self.sigs.adt(*item)?;
                if adt.builtin {
                    return None;
                }
                let env = adt_env(*item, args);
                adt.variants.iter().try_fold(0u64, |acc, variant| {
                    let fields = variant.fields.iter().try_fold(1u64, |acc, field| {
                        Some(acc.saturating_mul(self.size_at(&subst_ty(&field.ty, &env), depth)?))
                    })?;
                    Some(acc.saturating_add(fields))
                })
            }
            _ => None,
        }
    }

    /// Every value of `ty`, or `None` when `ty` is not finite or has more than `cap` values.
    pub fn values(&self, ty: &Ty, cap: u64) -> Option<Vec<ConstValue>> {
        if self.size(ty)? > cap {
            return None;
        }
        self.values_at(ty, 0)
    }

    fn values_at(&self, ty: &Ty, depth: u32) -> Option<Vec<ConstValue>> {
        if depth > MAX_DEPTH {
            return None;
        }
        let depth = depth.saturating_add(1);
        match ty.kind() {
            TyKind::Bool => Some(vec![ConstValue::Bool(false), ConstValue::Bool(true)]),
            TyKind::Bounded { lo, hi } => {
                let (lo, hi) = (lo.value()?, hi.value()?);
                let mut out = Vec::new();
                let mut value = lo;
                while value <= hi {
                    out.push(ConstValue::Int(value));
                    match value.checked_add(1) {
                        Some(next) => value = next,
                        None => break,
                    }
                }
                Some(out)
            }
            TyKind::Tuple(fields) => {
                let domains = fields
                    .iter()
                    .map(|field| self.values_at(field, depth))
                    .collect::<Option<Vec<_>>>()?;
                Some(
                    product(&domains)
                        .into_iter()
                        .map(|items| ConstValue::Tuple(items.into()))
                        .collect(),
                )
            }
            TyKind::Array { elem, len } => {
                let len = usize::try_from(len.value()?).ok()?;
                let elems = self.values_at(elem, depth)?;
                let domains = vec![elems; len];
                Some(
                    product(&domains)
                        .into_iter()
                        .map(|items| ConstValue::Array(items.into()))
                        .collect(),
                )
            }
            TyKind::Ref { inner, .. } => self.values_at(inner, depth),
            TyKind::FnPtr { kind, params, ret } => Some(
                self.functions_of(*kind, params, ret)?
                    .into_iter()
                    .map(|item| {
                        ConstValue::Fn(FnRef {
                            def: FnDefId::Item(item),
                            self_ty: None,
                            args: GenericArgs::empty(),
                        })
                    })
                    .collect(),
            ),
            TyKind::Adt { item, args } => {
                if *item == self.std.text {
                    return Some(self.texts.iter().cloned().map(ConstValue::Text).collect());
                }
                if let Some(kind) = self.std.asset_kind(*item) {
                    return Some(self.assets.get(&kind).cloned().unwrap_or_default());
                }
                let adt = self.sigs.adt(*item)?;
                if adt.builtin {
                    return None;
                }
                let env = adt_env(*item, args);
                let mut out = Vec::new();
                for (index, variant) in adt.variants.iter().enumerate() {
                    let domains = variant
                        .fields
                        .iter()
                        .map(|field| self.values_at(&subst_ty(&field.ty, &env), depth))
                        .collect::<Option<Vec<_>>>()?;
                    let variant = u32::try_from(index).ok()?;
                    out.extend(product(&domains).into_iter().map(|fields| ConstValue::Adt {
                        item: *item,
                        variant,
                        fields: fields.into(),
                    }));
                }
                Some(out)
            }
            _ => None,
        }
    }

    /// The named functions a pointer of this type can hold; `None` for `fn` pointers, which
    /// are not finite.
    fn functions_of(&self, kind: FnPtrKind, params: &[Ty], ret: &Ty) -> Option<Vec<ItemId>> {
        let matches_kind = |sig: &FnSig| match kind {
            FnPtrKind::Const => sig.kind == FnKind::Const,
            FnPtrKind::Pushdown => sig.kind == FnKind::Pushdown && !sig.scene,
            FnPtrKind::Scene => sig.kind == FnKind::Pushdown && sig.scene,
            FnPtrKind::Plain => false,
        };
        if kind == FnPtrKind::Plain {
            return None;
        }
        Some(
            self.functions
                .iter()
                .filter(|(_, sig)| matches_kind(sig) && sig.params == params && sig.ret == *ret)
                .map(|(item, _)| *item)
                .collect(),
        )
    }
}

/// The generic environment of an ADT's field types.
fn adt_env(item: ItemId, args: &[GenericArg]) -> GenericEnv {
    GenericEnv {
        item: Some(item),
        parent: None,
        parent_count: 0,
        args: args.to_vec(),
    }
}

/// The cartesian product of `domains`, the last varying fastest.
pub fn product(domains: &[Vec<ConstValue>]) -> Vec<Vec<ConstValue>> {
    let mut out: Vec<Vec<ConstValue>> = vec![Vec::new()];
    for domain in domains {
        let mut next = Vec::with_capacity(out.len().saturating_mul(domain.len()));
        for prefix in &out {
            for value in domain {
                let mut row = prefix.clone();
                row.push(value.clone());
                next.push(row);
            }
        }
        out = next;
    }
    out
}
