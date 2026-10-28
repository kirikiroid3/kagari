//! Finite types (`docs/reference/en/src/concepts/kinds.md#finite-types`,
//! `docs/dev/src/std/contract.md#finite-types`): the types the story may hold at runtime.

use crate::hir::FnPtrKind;
use crate::item_tree::{FnKind, ItemId};
use crate::signatures::subst::{Subst, subst_ty};
use crate::ty::{FnDefId, GenericArg, GenericArgs, Ty, TyKind};

use super::Env;

/// How deep the structural recursion through ADTs goes before a type counts as infinite.
const MAX_DEPTH: usize = 64;

impl Env<'_> {
    /// Whether the story may hold a runtime value of `ty`. `top_ref`: a reference is allowed
    /// at the top (a parameter or an argument); nested references never are. Closure types
    /// are not finite; where second-class closures are allowed, callers handle them first.
    ///
    /// Finite: `bool`, `()`, `!`, `Int<lo, hi>`, enums and structs whose fields are finite
    /// (generic arguments substituted), tuples and arrays of finite types, `Text` and the
    /// asset types, `pushdown fn`, `scene fn` and `const fn` pointers and named functions of
    /// those kinds. Rust integers and every other `#[builtin]` type are not; `EntityId`,
    /// `Part`, `Mark`, `Duration` and `Color` hold Rust integers. Generic parameters count as
    /// finite (generic code is checked where it is written, with finite arguments assumed),
    /// and so do erroneous types, which were reported already.
    pub(super) fn is_finite(&self, ty: &Ty, top_ref: bool) -> bool {
        let mut visiting = Vec::new();
        self.finite_in(ty, top_ref, &mut visiting)
    }

    fn finite_in(&self, ty: &Ty, top_ref: bool, visiting: &mut Vec<Ty>) -> bool {
        match ty.kind() {
            TyKind::Error
            | TyKind::Never
            | TyKind::Bool
            | TyKind::Bounded { .. }
            | TyKind::Param { .. }
            | TyKind::TraitSelf { .. }
            | TyKind::Projection { .. }
            | TyKind::Infer(_) => true,
            TyKind::Int(_) | TyKind::Closure { .. } => false,
            TyKind::Tuple(items) => items
                .iter()
                .all(|item| self.finite_in(item, false, visiting)),
            TyKind::Array { elem, .. } => self.finite_in(elem, false, visiting),
            TyKind::Ref { inner, .. } => top_ref && self.finite_in(inner, false, visiting),
            TyKind::FnPtr { kind, .. } => *kind != FnPtrKind::Plain,
            TyKind::FnDef(fn_ref) => match fn_ref.def {
                FnDefId::Item(item) => self
                    .sigs
                    .fn_sig(item)
                    .is_none_or(|sig| sig.kind != FnKind::Plain),
                FnDefId::StructCtor(_) | FnDefId::VariantCtor(..) => true,
            },
            TyKind::Adt { item, args } => {
                let Some(adt) = self.sigs.adt(*item) else {
                    return true;
                };
                if adt.builtin {
                    let std = &self.sigs.std;
                    return std.text == Some(*item) || std.assets.contains(item);
                }
                // A type met again inside itself is infinite.
                self.fields_all(*item, args, visiting, false, &mut |env, field, visiting| {
                    env.finite_in(field, false, visiting)
                })
            }
        }
    }

    /// Whether `ty` contains `Text`, looking through ADT fields.
    pub(super) fn contains_text(&self, ty: &Ty) -> bool {
        let text = self.sigs.std.text;
        self.contains(
            ty,
            &mut Vec::new(),
            &|ty| matches!(ty.kind(), TyKind::Adt { item, .. } if Some(*item) == text),
        )
    }

    /// Whether `ty` contains a reference, looking through ADT fields and generic arguments
    /// but not into function signatures.
    pub(super) fn contains_ref(&self, ty: &Ty) -> bool {
        self.contains(ty, &mut Vec::new(), &|ty| {
            matches!(ty.kind(), TyKind::Ref { .. })
        })
    }

    /// Whether `ty` contains a closure type, looking through ADT fields and generic
    /// arguments but not into function signatures.
    pub(super) fn contains_closure(&self, ty: &Ty) -> bool {
        self.contains(ty, &mut Vec::new(), &|ty| {
            matches!(ty.kind(), TyKind::Closure { .. })
        })
    }

    /// Whether `ty` contains a `pushdown fn` pointer (which holds an instant procedure).
    pub(super) fn contains_instant_pointer(&self, ty: &Ty) -> bool {
        self.contains(ty, &mut Vec::new(), &|ty| {
            matches!(
                ty.kind(),
                TyKind::FnPtr {
                    kind: FnPtrKind::Pushdown,
                    ..
                }
            )
        })
    }

    fn contains(&self, ty: &Ty, visiting: &mut Vec<Ty>, hit: &dyn Fn(&Ty) -> bool) -> bool {
        if hit(ty) {
            return true;
        }
        match ty.kind() {
            TyKind::Tuple(items) => items.iter().any(|item| self.contains(item, visiting, hit)),
            TyKind::Array { elem, .. } | TyKind::Ref { inner: elem, .. } => {
                self.contains(elem, visiting, hit)
            }
            TyKind::Adt { item, args } => {
                let in_args = args.iter().any(|arg| match arg {
                    GenericArg::Type(arg) => self.contains(arg, visiting, hit),
                    GenericArg::Const(_) => false,
                });
                in_args
                    || (self.sigs.adt(*item).is_some_and(|adt| !adt.builtin)
                        && !self.fields_all(
                            *item,
                            args,
                            visiting,
                            true,
                            &mut |env, field, visiting| !env.contains(field, visiting, hit),
                        ))
            }
            _ => false,
        }
    }

    /// Whether `check` holds for every field of every variant of the ADT `item<args>`;
    /// `on_cycle` when the type is met again inside itself (or nested too deeply).
    fn fields_all(
        &self,
        item: ItemId,
        args: &[GenericArg],
        visiting: &mut Vec<Ty>,
        on_cycle: bool,
        check: &mut dyn FnMut(&Self, &Ty, &mut Vec<Ty>) -> bool,
    ) -> bool {
        let Some(adt) = self.sigs.adt(item) else {
            return true;
        };
        let key = Ty::adt(item, GenericArgs::new(args.to_vec()));
        if visiting.contains(&key) || visiting.len() > MAX_DEPTH {
            return on_cycle;
        }
        visiting.push(key);
        let subst = Subst {
            owners: &[(item, 0)],
            args,
            self_ty: None,
        };
        let all = adt.variants.iter().all(|variant| {
            variant
                .fields
                .iter()
                .all(|field| check(self, &subst_ty(&field.ty, &subst), visiting))
        });
        visiting.pop();
        all
    }
}
