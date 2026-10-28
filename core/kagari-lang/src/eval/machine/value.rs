//! The values the machine computes with, and the generic environment of an activation.

use crate::hir::ExprId;
use crate::input::FilePath;
use crate::item_tree::ItemId;
use crate::mir::MirSource;
use crate::signatures::{ItemSig, ProgramSignatures};
use crate::ty::{Const, FnRef, GenericArg, GenericArgs, IntTy, Ty, TyKind};

use kagari_num::{Decimal, Rat};

use super::super::value::{AssetKind, HandleKind, Text};
use super::Place;
use super::screen::{PendingChange, Screen};

/// A value during evaluation.
#[derive(Clone, Debug)]
pub(crate) enum Value {
    Bool(bool),
    Int(i128),
    Decimal(Decimal),
    Rat(Rat),
    Text(Text),
    String(std::sync::Arc<str>),
    Tuple(imbl::Vector<Value>),
    Array(imbl::Vector<Value>),
    Vec(imbl::Vector<Value>),
    Adt {
        item: ItemId,
        variant: u32,
        fields: imbl::Vector<Value>,
    },
    Asset {
        kind: AssetKind,
        path: FilePath,
    },
    Fn(FnRef),
    Closure(Box<ClosureValue>),
    /// A path to a handle `static`.
    Handle {
        kind: HandleKind,
        item: ItemId,
    },
    /// `setting(init)` / `meta(init)`: a handle being created by a `static` initializer.
    NewHandle {
        kind: HandleKind,
        init: Box<Value>,
    },
    Screen(std::sync::Arc<Screen>),
    Ref(Box<Place>),
    /// A `Change<T>`, `Swap` or `ChannelChange` waiting for `apply_animation`.
    Pending(Box<Pending>),
    /// The opaque input screen of a render's keyframe test: inspecting it in any way aborts
    /// with [`super::ErrorKind::Inspected`].
    Probe,
    /// A `let` without initializer.
    Uninit,
}

impl Value {
    pub fn unit() -> Value {
        Value::Tuple(imbl::Vector::new())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ClosureValue {
    /// The closure's body.
    pub source: MirSource,
    pub env: GenericEnv,
    /// The captured values, in the order of the closure's captures.
    pub captures: Vec<Value>,
}

impl ClosureValue {
    /// The closure `expr` of `owner` without captures, in a non-generic context.
    pub fn plain(owner: ItemId, expr: ExprId) -> ClosureValue {
        ClosureValue {
            source: MirSource::Closure { owner, expr },
            env: GenericEnv::default(),
            captures: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Pending {
    /// The screen the change applies to.
    pub screen: Place,
    pub change: PendingChange,
}

/// The generic arguments of the function or constant being evaluated.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GenericEnv {
    pub item: Option<ItemId>,
    pub parent: Option<ItemId>,
    pub parent_count: u32,
    pub args: Vec<GenericArg>,
}

impl GenericEnv {
    pub(crate) fn arg(&self, owner: ItemId, index: u32) -> Option<&GenericArg> {
        let position = if Some(owner) == self.item {
            self.parent_count.checked_add(index)?
        } else if Some(owner) == self.parent {
            index
        } else {
            return None;
        };
        self.args.get(position as usize)
    }
}

pub(crate) fn subst_const(value: &Const, env: &GenericEnv) -> Const {
    match value {
        Const::Param { owner, index } => match env.arg(*owner, *index) {
            Some(GenericArg::Const(value)) => value.clone(),
            _ => value.clone(),
        },
        _ => value.clone(),
    }
}

pub(crate) fn subst_arg(arg: &GenericArg, env: &GenericEnv) -> GenericArg {
    match arg {
        GenericArg::Type(ty) => GenericArg::Type(subst_ty(ty, env)),
        GenericArg::Const(value) => GenericArg::Const(subst_const(value, env)),
    }
}

pub(crate) fn subst_fn_ref(fn_ref: &FnRef, env: &GenericEnv) -> FnRef {
    if env.args.is_empty() {
        return fn_ref.clone();
    }
    FnRef {
        def: fn_ref.def,
        self_ty: fn_ref.self_ty.as_ref().map(|ty| subst_ty(ty, env)),
        args: fn_ref.args.iter().map(|arg| subst_arg(arg, env)).collect(),
    }
}

/// `ty` with the generic parameters of `env` replaced by their arguments.
pub(crate) fn subst_ty(ty: &Ty, env: &GenericEnv) -> Ty {
    if env.args.is_empty() || !ty.has_param() {
        return ty.clone();
    }
    let all = |tys: &[Ty]| tys.iter().map(|ty| subst_ty(ty, env)).collect::<Box<[_]>>();
    match ty.kind() {
        TyKind::Param { owner, index } => match env.arg(*owner, *index) {
            Some(GenericArg::Type(arg)) => arg.clone(),
            _ => ty.clone(),
        },
        TyKind::Bounded { lo, hi } => Ty::new(TyKind::Bounded {
            lo: subst_const(lo, env),
            hi: subst_const(hi, env),
        }),
        TyKind::Tuple(fields) => Ty::new(TyKind::Tuple(all(fields))),
        TyKind::Array { elem, len } => Ty::new(TyKind::Array {
            elem: subst_ty(elem, env),
            len: subst_const(len, env),
        }),
        TyKind::Adt { item, args } => Ty::new(TyKind::Adt {
            item: *item,
            args: args.iter().map(|arg| subst_arg(arg, env)).collect(),
        }),
        TyKind::Ref { mutable, inner } => Ty::new(TyKind::Ref {
            mutable: *mutable,
            inner: subst_ty(inner, env),
        }),
        TyKind::FnPtr { kind, params, ret } => Ty::new(TyKind::FnPtr {
            kind: *kind,
            params: all(params),
            ret: subst_ty(ret, env),
        }),
        TyKind::Closure { kind, params, ret } => Ty::new(TyKind::Closure {
            kind: *kind,
            params: all(params),
            ret: subst_ty(ret, env),
        }),
        TyKind::FnDef(fn_ref) => Ty::new(TyKind::FnDef(subst_fn_ref(fn_ref, env))),
        TyKind::Projection {
            base,
            trait_item,
            name,
        } => Ty::new(TyKind::Projection {
            base: subst_ty(base, env),
            trait_item: *trait_item,
            name: name.clone(),
        }),
        TyKind::Error
        | TyKind::Never
        | TyKind::Bool
        | TyKind::Int(_)
        | TyKind::TraitSelf { .. }
        | TyKind::Infer(_) => ty.clone(),
    }
}

/// The `[lo, hi]` range of an integer type, when known.
pub(crate) fn int_range(ty: &Ty) -> Option<(i128, i128)> {
    match ty.kind() {
        TyKind::Int(int) => Some((IntTy::min(*int), IntTy::max(*int))),
        TyKind::Bounded { lo, hi } => Some((lo.value()?, hi.value()?)),
        _ => None,
    }
}

/// `u32`, `Int<0, 5>`, … for messages.
pub(crate) fn int_ty_name(ty: &Ty) -> String {
    match ty.kind() {
        TyKind::Int(int) => int.name().to_owned(),
        TyKind::Bounded { lo, hi } => match (lo.value(), hi.value()) {
            (Some(lo), Some(hi)) => format!("Int<{lo}, {hi}>"),
            _ => "Int".to_owned(),
        },
        _ => "integer".to_owned(),
    }
}

impl GenericEnv {
    /// The environment of `item` instantiated with `args` (its parent's arguments first).
    pub fn for_item(sigs: &ProgramSignatures, item: ItemId, args: GenericArgs) -> GenericEnv {
        let generics = match sigs.get(item) {
            Some(ItemSig::Fn(sig)) => Some(&sig.generics),
            Some(ItemSig::Const { generics, .. }) => Some(generics),
            _ => None,
        };
        GenericEnv {
            item: Some(item),
            parent: generics.and_then(|generics| generics.parent),
            parent_count: generics.map_or(0, |generics| generics.parent_count),
            args: args.to_vec(),
        }
    }
}
