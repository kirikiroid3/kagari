//! Substitution of generic parameters and matching of generic types against concrete ones.

use crate::item_tree::ItemId;
use crate::ty::{Const, FnRef, GenericArg, GenericArgs, Ty, TyKind};

/// Generic parameters to replace: the parameters of each owner start at its offset in `args`.
pub(crate) struct Subst<'a> {
    pub owners: &'a [(ItemId, usize)],
    pub args: &'a [GenericArg],
    /// What `Self` of a trait (`Ty::TraitSelf`) stands for.
    pub self_ty: Option<&'a Ty>,
}

impl Subst<'_> {
    fn arg(&self, owner: ItemId, index: u32) -> Option<&GenericArg> {
        let (_, offset) = self.owners.iter().find(|(item, _)| *item == owner)?;
        self.args.get(offset.checked_add(index as usize)?)
    }
}

pub(crate) fn subst_ty(ty: &Ty, subst: &Subst<'_>) -> Ty {
    if !ty.has_param() {
        return ty.clone();
    }
    match ty.kind() {
        TyKind::Param { owner, index } => match subst.arg(*owner, *index) {
            Some(GenericArg::Type(arg)) => arg.clone(),
            Some(GenericArg::Const(_)) => Ty::error(),
            None => ty.clone(),
        },
        TyKind::TraitSelf { .. } => match subst.self_ty {
            Some(self_ty) => self_ty.clone(),
            None => ty.clone(),
        },
        TyKind::Error | TyKind::Never | TyKind::Bool | TyKind::Int(_) | TyKind::Infer(_) => {
            ty.clone()
        }
        TyKind::Bounded { lo, hi } => Ty::new(TyKind::Bounded {
            lo: subst_const(lo, subst),
            hi: subst_const(hi, subst),
        }),
        TyKind::Tuple(fields) => Ty::new(TyKind::Tuple(
            fields.iter().map(|field| subst_ty(field, subst)).collect(),
        )),
        TyKind::Array { elem, len } => Ty::new(TyKind::Array {
            elem: subst_ty(elem, subst),
            len: subst_const(len, subst),
        }),
        TyKind::Adt { item, args } => Ty::new(TyKind::Adt {
            item: *item,
            args: subst_args(args, subst),
        }),
        TyKind::Ref { mutable, inner } => Ty::new(TyKind::Ref {
            mutable: *mutable,
            inner: subst_ty(inner, subst),
        }),
        TyKind::FnPtr { kind, params, ret } => Ty::new(TyKind::FnPtr {
            kind: *kind,
            params: params.iter().map(|param| subst_ty(param, subst)).collect(),
            ret: subst_ty(ret, subst),
        }),
        TyKind::Closure { kind, params, ret } => Ty::new(TyKind::Closure {
            kind: *kind,
            params: params.iter().map(|param| subst_ty(param, subst)).collect(),
            ret: subst_ty(ret, subst),
        }),
        TyKind::FnDef(fn_ref) => Ty::new(TyKind::FnDef(subst_fn_ref(fn_ref, subst))),
        TyKind::Projection {
            base,
            trait_item,
            name,
        } => Ty::new(TyKind::Projection {
            base: subst_ty(base, subst),
            trait_item: *trait_item,
            name: name.clone(),
        }),
    }
}

pub(crate) fn subst_const(c: &Const, subst: &Subst<'_>) -> Const {
    match c {
        Const::Param { owner, index } => match subst.arg(*owner, *index) {
            Some(GenericArg::Const(arg)) => arg.clone(),
            Some(GenericArg::Type(_)) => Const::Error,
            None => c.clone(),
        },
        _ => c.clone(),
    }
}

pub(crate) fn subst_args(args: &[GenericArg], subst: &Subst<'_>) -> GenericArgs {
    args.iter()
        .map(|arg| match arg {
            GenericArg::Type(ty) => GenericArg::Type(subst_ty(ty, subst)),
            GenericArg::Const(c) => GenericArg::Const(subst_const(c, subst)),
        })
        .collect()
}

pub(crate) fn subst_fn_ref(fn_ref: &FnRef, subst: &Subst<'_>) -> FnRef {
    FnRef {
        def: fn_ref.def,
        self_ty: fn_ref.self_ty.as_ref().map(|ty| subst_ty(ty, subst)),
        args: subst_args(&fn_ref.args, subst),
    }
}

/// Matches `pattern`, a type mentioning the generic parameters of `owner`, against `ty`,
/// binding the parameters in `out`. Inference variables and errors in `ty` match anything.
pub(crate) fn match_ty(
    pattern: &Ty,
    ty: &Ty,
    owner: ItemId,
    out: &mut [Option<GenericArg>],
) -> bool {
    if let TyKind::Param {
        owner: param_owner,
        index,
    } = pattern.kind()
        && *param_owner == owner
    {
        return bind(out, *index, GenericArg::Type(ty.clone()));
    }
    if matches!(ty.kind(), TyKind::Infer(_) | TyKind::Error) {
        return true;
    }
    match (pattern.kind(), ty.kind()) {
        (TyKind::Error, _) => true,
        (TyKind::Never, TyKind::Never) | (TyKind::Bool, TyKind::Bool) => true,
        (TyKind::Int(a), TyKind::Int(b)) => a == b,
        (TyKind::Bounded { lo: l1, hi: h1 }, TyKind::Bounded { lo: l2, hi: h2 }) => {
            match_const(l1, l2, owner, out) && match_const(h1, h2, owner, out)
        }
        (TyKind::Tuple(a), TyKind::Tuple(b)) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b.iter())
                    .all(|(pattern, ty)| match_ty(pattern, ty, owner, out))
        }
        (TyKind::Array { elem: e1, len: l1 }, TyKind::Array { elem: e2, len: l2 }) => {
            match_ty(e1, e2, owner, out) && match_const(l1, l2, owner, out)
        }
        (TyKind::Adt { item: i1, args: a1 }, TyKind::Adt { item: i2, args: a2 }) => {
            i1 == i2 && match_args(a1, a2, owner, out)
        }
        (
            TyKind::Ref {
                mutable: m1,
                inner: i1,
            },
            TyKind::Ref {
                mutable: m2,
                inner: i2,
            },
        ) => m1 == m2 && match_ty(i1, i2, owner, out),
        (
            TyKind::FnPtr {
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
            k1 == k2
                && p1.len() == p2.len()
                && p1
                    .iter()
                    .zip(p2.iter())
                    .all(|(a, b)| match_ty(a, b, owner, out))
                && match_ty(r1, r2, owner, out)
        }
        (
            TyKind::Closure {
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
            k1 == k2
                && p1.len() == p2.len()
                && p1
                    .iter()
                    .zip(p2.iter())
                    .all(|(a, b)| match_ty(a, b, owner, out))
                && match_ty(r1, r2, owner, out)
        }
        _ => pattern == ty,
    }
}

fn match_args(
    patterns: &[GenericArg],
    args: &[GenericArg],
    owner: ItemId,
    out: &mut [Option<GenericArg>],
) -> bool {
    patterns.len() == args.len()
        && patterns.iter().zip(args).all(|pair| match pair {
            (GenericArg::Type(a), GenericArg::Type(b)) => match_ty(a, b, owner, out),
            (GenericArg::Const(a), GenericArg::Const(b)) => match_const(a, b, owner, out),
            _ => false,
        })
}

fn match_const(pattern: &Const, c: &Const, owner: ItemId, out: &mut [Option<GenericArg>]) -> bool {
    match (pattern, c) {
        (
            Const::Param {
                owner: param_owner,
                index,
            },
            _,
        ) if *param_owner == owner => bind(out, *index, GenericArg::Const(c.clone())),
        (_, Const::Infer(_) | Const::Error) | (Const::Error, _) => true,
        _ => pattern == c,
    }
}

fn is_wildcard(arg: &GenericArg) -> bool {
    match arg {
        GenericArg::Type(ty) => matches!(ty.kind(), TyKind::Error | TyKind::Infer(_)),
        GenericArg::Const(c) => matches!(c, Const::Error | Const::Infer(_)),
    }
}

fn bind(out: &mut [Option<GenericArg>], index: u32, arg: GenericArg) -> bool {
    match out.get_mut(index as usize) {
        Some(slot @ None) => {
            *slot = Some(arg);
            true
        }
        Some(Some(existing)) if is_wildcard(existing) => {
            *existing = arg;
            true
        }
        Some(Some(existing)) => *existing == arg || is_wildcard(&arg),
        None => false,
    }
}
