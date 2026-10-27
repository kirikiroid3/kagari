//! Places: mutability of assigned and mutably borrowed places (values.md#references) and
//! exclusive `&mut` arguments in the story.

use crate::diagnostic::DiagnosticCode;
use crate::hir::{Expr, ExprId, UnaryOp};
use crate::infer::{Coercion, ReceiverAdjust, ValueRef};
use crate::item_tree::ItemId;
use crate::resolve::LocalId;
use crate::signatures::ItemSig;
use crate::ty::{Ty, TyKind};

use super::body::BodyChecker;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Access {
    Assign,
    Borrow,
}

/// An argument of a call, for the exclusivity rule.
#[derive(Clone, Copy, Debug)]
pub(super) enum CallArg {
    Value(ExprId),
    Receiver(ExprId, ReceiverAdjust),
}

/// Whether a place may be changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlaceMut {
    Mutable,
    /// Not a place: a temporary value.
    Temporary,
    /// A place of a local not declared `mut`.
    Local(LocalId),
    /// An immutable `static`.
    Static(ItemId),
    Const(ItemId),
    /// Reached through a `&T`.
    Shared,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Root {
    Local(LocalId),
    Static(ItemId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Proj {
    Field(u32),
    Index,
    Deref,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Place {
    root: Root,
    proj: Vec<Proj>,
}

impl Place {
    fn deref(mut self, count: u32) -> Place {
        for _ in 0..count {
            self.proj.push(Proj::Deref);
        }
        self
    }

    /// Two places overlap when one is inside the other; any two elements may be the same.
    fn overlaps(&self, other: &Place) -> bool {
        self.root == other.root
            && self
                .proj
                .iter()
                .zip(&other.proj)
                .all(|(a, b)| match (a, b) {
                    (Proj::Field(a), Proj::Field(b)) => a == b,
                    (Proj::Index, Proj::Index) | (Proj::Deref, Proj::Deref) => true,
                    _ => false,
                })
    }
}

/// The number of references `ty` is behind, and whether they are all `&mut`.
fn ref_chain(mut ty: &Ty, count: u32) -> bool {
    for _ in 0..count {
        match ty.kind() {
            TyKind::Ref { mutable, inner } => {
                if !mutable {
                    return false;
                }
                ty = inner;
            }
            _ => return true,
        }
    }
    true
}

fn ref_depth(mut ty: &Ty) -> u32 {
    let mut depth = 0u32;
    while let TyKind::Ref { inner, .. } = ty.kind() {
        depth = depth.saturating_add(1);
        ty = inner;
    }
    depth
}

impl BodyChecker<'_, '_> {
    fn place(&self, expr: ExprId) -> Option<Place> {
        match self.store.expr(expr) {
            Expr::Path(_) => match self.infer.paths.get(&expr)? {
                ValueRef::Local(local) => Some(Place {
                    root: Root::Local(*local),
                    proj: Vec::new(),
                }),
                ValueRef::Static(item) => Some(Place {
                    root: Root::Static(*item),
                    proj: Vec::new(),
                }),
                _ => None,
            },
            Expr::Field { base, .. } => {
                let field = self.infer.fields.get(&expr)?;
                let mut place = self.place(*base)?.deref(field.derefs);
                place.proj.push(Proj::Field(field.index));
                Some(place)
            }
            Expr::Index { base, .. } => {
                let depth = ref_depth(self.ty(*base)?);
                let mut place = self.place(*base)?.deref(depth);
                place.proj.push(Proj::Index);
                Some(place)
            }
            Expr::Unary {
                op: UnaryOp::Deref,
                expr: inner,
            } => Some(self.place(*inner)?.deref(1)),
            _ => None,
        }
    }

    fn place_mut(&self, expr: ExprId) -> PlaceMut {
        match self.store.expr(expr) {
            Expr::Path(_) => match self.infer.paths.get(&expr) {
                Some(ValueRef::Local(local)) => {
                    if self.res.locals[*local].mutable {
                        PlaceMut::Mutable
                    } else {
                        PlaceMut::Local(*local)
                    }
                }
                Some(ValueRef::Static(item)) => match self.env.sigs.get(*item) {
                    Some(ItemSig::Static { mutable: false, .. }) => PlaceMut::Static(*item),
                    _ => PlaceMut::Mutable,
                },
                Some(ValueRef::Const { item, .. }) => PlaceMut::Const(*item),
                Some(ValueRef::Error) | None => PlaceMut::Mutable,
                Some(_) => PlaceMut::Temporary,
            },
            Expr::Field { base, .. } => match self.infer.fields.get(&expr) {
                Some(field) if field.derefs > 0 => self.through(*base, field.derefs),
                Some(_) => self.place_mut(*base),
                None => PlaceMut::Mutable,
            },
            Expr::Index { base, .. } => match self.ty(*base) {
                Some(ty) if ref_depth(ty) > 0 => self.through(*base, ref_depth(ty)),
                Some(_) => self.place_mut(*base),
                None => PlaceMut::Mutable,
            },
            Expr::Unary {
                op: UnaryOp::Deref,
                expr: inner,
            } => self.through(*inner, 1),
            Expr::Missing => PlaceMut::Mutable,
            _ => PlaceMut::Temporary,
        }
    }

    /// A place reached by dereferencing `base` `count` times.
    fn through(&self, base: ExprId, count: u32) -> PlaceMut {
        match self.ty(base) {
            Some(ty) if !ref_chain(ty, count) => PlaceMut::Shared,
            _ => PlaceMut::Mutable,
        }
    }

    /// Assignment and `&mut` need a mutable place (values.md#references).
    pub(super) fn check_mut_place(&mut self, target: ExprId, access: Access) {
        if self.ty(target).is_none() {
            return;
        }
        let mut anchors = Vec::new();
        let direct = matches!(self.store.expr(target), Expr::Path(_));
        let (message, binding) = match (self.place_mut(target), access) {
            (PlaceMut::Mutable, _) | (PlaceMut::Temporary | PlaceMut::Const(_), Access::Borrow) => {
                return;
            }
            (PlaceMut::Temporary, Access::Assign) => (
                format!(
                    "invalid left-hand side of assignment: `{}` is a temporary value",
                    self.src(&mut anchors, target)
                ),
                None,
            ),
            (PlaceMut::Const(item), Access::Assign) => (
                format!("cannot assign to constant `{}`", self.env.item_name(item)),
                None,
            ),
            (PlaceMut::Static(item), Access::Assign) => (
                format!(
                    "cannot assign to immutable static item `{}`",
                    self.env.item_name(item)
                ),
                None,
            ),
            (PlaceMut::Static(item), Access::Borrow) => (
                format!(
                    "cannot borrow immutable static item `{}` as mutable",
                    self.env.item_name(item)
                ),
                None,
            ),
            (PlaceMut::Shared, Access::Assign) => (
                format!(
                    "cannot assign to `{}`, which is behind a `&` reference",
                    self.src(&mut anchors, target)
                ),
                None,
            ),
            (PlaceMut::Shared, Access::Borrow) => (
                format!(
                    "cannot borrow `{}` as mutable, as it is behind a `&` reference",
                    self.src(&mut anchors, target)
                ),
                None,
            ),
            (PlaceMut::Local(local), access) => {
                let name = self.local_name(local);
                let message = match (access, direct) {
                    (Access::Assign, true) if self.params.contains(&local) => {
                        format!("cannot assign to immutable argument `{name}`")
                    }
                    (Access::Assign, true) => {
                        format!("cannot assign twice to immutable variable `{name}`")
                    }
                    (Access::Assign, false) => format!(
                        "cannot assign to `{}`, as `{name}` is not declared as mutable",
                        self.src(&mut anchors, target)
                    ),
                    (Access::Borrow, true) => {
                        format!(
                            "cannot borrow `{name}` as mutable, as it is not declared as mutable"
                        )
                    }
                    (Access::Borrow, false) => format!(
                        "cannot borrow `{}` as mutable, as `{name}` is not declared as mutable",
                        self.src(&mut anchors, target)
                    ),
                };
                (message, Some(local))
            }
        };
        let mut diag = self.diag(
            self.anchor(target),
            DiagnosticCode::KindImmutablePlace,
            message,
        );
        diag.message_anchors = anchors;
        if let Some(local) = binding
            && let Some(pat) = self.res.locals[local].pat
        {
            diag = diag.with_related(self.pat_anchor(pat), "consider changing this to be mutable");
        }
        self.report(diag);
    }

    /// A `&mut self` method called on `receiver`, dereferenced `derefs` times first.
    pub(super) fn check_receiver_mut(&mut self, receiver: ExprId, derefs: u32) {
        if derefs == 0 {
            self.check_mut_place(receiver, Access::Borrow);
            return;
        }
        if self.through(receiver, derefs) == PlaceMut::Shared {
            let mut anchors = Vec::new();
            let src = self.src(&mut anchors, receiver);
            let mut diag = self.diag(
                self.anchor(receiver),
                DiagnosticCode::KindImmutablePlace,
                format!("cannot borrow `*{src}` as mutable, as it is behind a `&` reference"),
            );
            diag.message_anchors = anchors;
            self.report(diag);
        }
    }

    /// The place an argument passes and whether it passes it as `&mut`.
    fn arg_place(&self, arg: CallArg) -> Option<(ExprId, Place, bool)> {
        match arg {
            CallArg::Receiver(expr, adjust) => {
                let place = self.place(expr)?.deref(adjust.derefs);
                Some((expr, place, adjust.autoref == Some(true)))
            }
            CallArg::Value(expr) => match self.store.expr(expr) {
                Expr::Ref {
                    mutable,
                    expr: inner,
                } => {
                    let shared = matches!(
                        self.infer.coercions.get(&expr),
                        Some((Coercion::MutToShared, _))
                    );
                    Some((expr, self.place(*inner)?, *mutable && !shared))
                }
                _ => {
                    let place = self.place(expr)?;
                    match self.ty(expr)?.kind() {
                        TyKind::Ref { mutable, .. } => {
                            let shared = matches!(
                                self.infer.coercions.get(&expr),
                                Some((Coercion::MutToShared, _))
                            );
                            Some((expr, place.deref(1), *mutable && !shared))
                        }
                        _ => Some((expr, place, false)),
                    }
                }
            },
        }
    }

    /// Exclusivity (values.md#references-in-the-story): a place passed as `&mut` is not
    /// passed as any other argument of the same call.
    pub(super) fn check_exclusive(&mut self, args: &[CallArg]) {
        let places: Vec<(ExprId, Place, bool)> =
            args.iter().filter_map(|&arg| self.arg_place(arg)).collect();
        for (later_index, (later, later_place, later_mut)) in places.iter().enumerate() {
            let earlier = places.iter().take(later_index).find(|(_, place, mutable)| {
                (*mutable || *later_mut) && place.overlaps(later_place)
            });
            let Some((first, _, first_mut)) = earlier else {
                continue;
            };
            let borrowed = if *first_mut { *first } else { *later };
            // The place itself, without the `&mut` written before it.
            let place = match self.store.expr(borrowed) {
                Expr::Ref { expr: inner, .. } => *inner,
                _ => borrowed,
            };
            let mut anchors = Vec::new();
            let mut diag = self
                .diag(
                    self.anchor(*later),
                    DiagnosticCode::KindAliasedMut,
                    format!(
                        "`{}` is passed as `&mut` and also as another argument of this call: in \
                         the story, a `&mut` argument cannot alias another argument",
                        self.src(&mut anchors, place)
                    ),
                )
                .with_related(self.anchor(*first), "the same place is also passed here");
            diag.message_anchors = anchors;
            self.report(diag);
        }
    }
}
