//! Inference variables and unification.

use crate::hir::ExprId;
use crate::ty::{Const, FnRef, GenericArg, InferVar, Ty, TyNode};

/// An integer literal whose type is not known yet: the hull of the values it must hold.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Literal {
    pub lo: i128,
    pub hi: i128,
    /// The literal expression, where an out-of-range value is reported.
    pub origin: ExprId,
}

#[derive(Clone, Debug)]
enum VarData {
    Ty {
        value: Option<Ty>,
        literal: Option<Literal>,
    },
    Const(Option<Const>),
}

/// A literal variable bound to an integer type: its values must fit in `ty`.
#[derive(Clone, Debug)]
pub(crate) struct Fit {
    pub literal: Literal,
    pub ty: Ty,
}

#[derive(Clone, Copy)]
pub(crate) struct Snapshot {
    undo: usize,
    fits: usize,
}

#[derive(Default)]
pub(crate) struct Table {
    vars: Vec<VarData>,
    undo: Vec<(usize, VarData)>,
    pub fits: Vec<Fit>,
}

fn index(var: InferVar) -> usize {
    var.0 as usize
}

impl Table {
    fn push(&mut self, data: VarData) -> InferVar {
        let var = InferVar(u32::try_from(self.vars.len()).unwrap_or(u32::MAX));
        self.vars.push(data);
        var
    }

    pub(crate) fn new_var(&mut self) -> Ty {
        Ty::Infer(self.push(VarData::Ty {
            value: None,
            literal: None,
        }))
    }

    pub(crate) fn new_literal(&mut self, lo: i128, hi: i128, origin: ExprId) -> Ty {
        Ty::Infer(self.push(VarData::Ty {
            value: None,
            literal: Some(Literal { lo, hi, origin }),
        }))
    }

    pub(crate) fn new_const(&mut self) -> Const {
        Const::Infer(self.push(VarData::Const(None)))
    }

    pub(crate) fn snapshot(&self) -> Snapshot {
        Snapshot {
            undo: self.undo.len(),
            fits: self.fits.len(),
        }
    }

    pub(crate) fn rollback(&mut self, snapshot: Snapshot) {
        while self.undo.len() > snapshot.undo {
            let Some((var, data)) = self.undo.pop() else {
                break;
            };
            if let Some(slot) = self.vars.get_mut(var) {
                *slot = data;
            }
        }
        self.fits.truncate(snapshot.fits);
    }

    fn set(&mut self, var: InferVar, data: VarData) {
        if let Some(slot) = self.vars.get_mut(index(var)) {
            let old = std::mem::replace(slot, data);
            self.undo.push((index(var), old));
        }
    }

    fn ty_value(&self, var: InferVar) -> Option<&Ty> {
        match self.vars.get(index(var)) {
            Some(VarData::Ty { value, .. }) => value.as_ref(),
            _ => None,
        }
    }

    /// The literal hull of an unresolved literal variable.
    pub(crate) fn literal_of(&self, var: InferVar) -> Option<Literal> {
        match self.vars.get(index(var)) {
            Some(VarData::Ty {
                value: None,
                literal,
            }) => *literal,
            _ => None,
        }
    }

    /// Whether `ty` is (after following bindings) an unresolved integer literal variable.
    pub(crate) fn literal(&self, ty: &Ty) -> Option<(InferVar, Literal)> {
        match self.shallow(ty) {
            Ty::Infer(var) => self.literal_of(var).map(|literal| (var, literal)),
            _ => None,
        }
    }

    pub(crate) fn is_literal_var(&self, var: InferVar) -> bool {
        self.literal_of(self.root(var)).is_some()
    }

    fn root(&self, mut var: InferVar) -> InferVar {
        while let Some(Ty::Infer(next)) = self.ty_value(var) {
            var = *next;
        }
        var
    }

    /// `ty` with the bindings of its head followed.
    pub(crate) fn shallow(&self, ty: &Ty) -> Ty {
        let mut current = ty;
        while let Ty::Infer(var) = current {
            match self.ty_value(*var) {
                Some(next) => current = next,
                None => break,
            }
        }
        current.clone()
    }

    pub(crate) fn shallow_const(&self, c: &Const) -> Const {
        let mut current = c.clone();
        while let Const::Infer(var) = current {
            match self.vars.get(index(var)) {
                Some(VarData::Const(Some(next))) => current = next.clone(),
                _ => break,
            }
        }
        current
    }

    /// `ty` with every bound variable replaced by its value.
    pub(crate) fn resolve(&self, ty: &Ty) -> Ty {
        match self.shallow(ty) {
            Ty::Infer(var) => Ty::Infer(var),
            Ty::Bounded { lo, hi } => Ty::Bounded {
                lo: self.shallow_const(&lo),
                hi: self.shallow_const(&hi),
            },
            Ty::Tuple(fields) => {
                Ty::Tuple(fields.iter().map(|field| self.resolve(field)).collect())
            }
            Ty::Array { elem, len } => Ty::Array {
                elem: Box::new(self.resolve(&elem)),
                len: self.shallow_const(&len),
            },
            Ty::Adt { item, args } => Ty::Adt {
                item,
                args: self.resolve_args(&args),
            },
            Ty::Ref { mutable, inner } => Ty::Ref {
                mutable,
                inner: Box::new(self.resolve(&inner)),
            },
            Ty::FnPtr { kind, params, ret } => Ty::FnPtr {
                kind,
                params: params.iter().map(|param| self.resolve(param)).collect(),
                ret: Box::new(self.resolve(&ret)),
            },
            Ty::Closure { kind, params, ret } => Ty::Closure {
                kind,
                params: params.iter().map(|param| self.resolve(param)).collect(),
                ret: Box::new(self.resolve(&ret)),
            },
            Ty::FnDef(fn_ref) => Ty::FnDef(Box::new(self.resolve_fn_ref(&fn_ref))),
            Ty::Projection {
                base,
                trait_item,
                name,
            } => Ty::Projection {
                base: Box::new(self.resolve(&base)),
                trait_item,
                name,
            },
            other => other,
        }
    }

    pub(crate) fn resolve_args(&self, args: &[GenericArg]) -> Vec<GenericArg> {
        args.iter()
            .map(|arg| match arg {
                GenericArg::Type(ty) => GenericArg::Type(self.resolve(ty)),
                GenericArg::Const(c) => GenericArg::Const(self.shallow_const(c)),
            })
            .collect()
    }

    pub(crate) fn resolve_fn_ref(&self, fn_ref: &FnRef) -> FnRef {
        FnRef {
            def: fn_ref.def,
            self_ty: fn_ref.self_ty.as_ref().map(|ty| self.resolve(ty)),
            args: self.resolve_args(&fn_ref.args),
        }
    }

    pub(crate) fn unify(&mut self, a: &Ty, b: &Ty) -> bool {
        let a = self.shallow(a);
        let b = self.shallow(b);
        match (&a, &b) {
            (Ty::Error, _) | (_, Ty::Error) => true,
            (Ty::Infer(x), Ty::Infer(y)) if x == y => true,
            (Ty::Infer(x), Ty::Infer(y)) => {
                self.union(*x, *y);
                true
            }
            (Ty::Infer(var), other) | (other, Ty::Infer(var)) => self.bind(*var, other),
            (Ty::Never, Ty::Never) | (Ty::Bool, Ty::Bool) => true,
            (Ty::Int(x), Ty::Int(y)) => x == y,
            (Ty::Bounded { lo: l1, hi: h1 }, Ty::Bounded { lo: l2, hi: h2 }) => {
                self.unify_const(l1, l2) && self.unify_const(h1, h2)
            }
            (Ty::Tuple(xs), Ty::Tuple(ys)) => {
                xs.len() == ys.len() && xs.iter().zip(ys).all(|(x, y)| self.unify(x, y))
            }
            (Ty::Array { elem: e1, len: l1 }, Ty::Array { elem: e2, len: l2 }) => {
                self.unify(e1, e2) && self.unify_const(l1, l2)
            }
            (Ty::Adt { item: i1, args: a1 }, Ty::Adt { item: i2, args: a2 }) => {
                i1 == i2 && self.unify_args(a1, a2)
            }
            (
                Ty::Ref {
                    mutable: m1,
                    inner: i1,
                },
                Ty::Ref {
                    mutable: m2,
                    inner: i2,
                },
            ) => m1 == m2 && self.unify(i1, i2),
            (
                Ty::FnPtr {
                    kind: k1,
                    params: p1,
                    ret: r1,
                },
                Ty::FnPtr {
                    kind: k2,
                    params: p2,
                    ret: r2,
                },
            ) => k1 == k2 && self.unify_sig(p1, r1, p2, r2),
            (
                Ty::Closure {
                    kind: k1,
                    params: p1,
                    ret: r1,
                },
                Ty::Closure {
                    kind: k2,
                    params: p2,
                    ret: r2,
                },
            ) => k1 == k2 && self.unify_sig(p1, r1, p2, r2),
            (Ty::FnDef(f), Ty::FnDef(g)) => {
                f.def == g.def
                    && self.unify_args(&f.args, &g.args)
                    && match (&f.self_ty, &g.self_ty) {
                        (Some(x), Some(y)) => self.unify(x, y),
                        (None, None) => true,
                        _ => false,
                    }
            }
            (
                Ty::Projection {
                    base: b1,
                    trait_item: t1,
                    name: n1,
                },
                Ty::Projection {
                    base: b2,
                    trait_item: t2,
                    name: n2,
                },
            ) => t1 == t2 && n1 == n2 && self.unify(b1, b2),
            _ => a == b,
        }
    }

    pub(crate) fn unify_sig(&mut self, p1: &[Ty], r1: &Ty, p2: &[Ty], r2: &Ty) -> bool {
        p1.len() == p2.len()
            && p1.iter().zip(p2).all(|(x, y)| self.unify(x, y))
            && self.unify(r1, r2)
    }

    pub(crate) fn unify_args(&mut self, a: &[GenericArg], b: &[GenericArg]) -> bool {
        a.len() == b.len()
            && a.iter().zip(b).all(|pair| match pair {
                (GenericArg::Type(x), GenericArg::Type(y)) => self.unify(x, y),
                (GenericArg::Const(x), GenericArg::Const(y)) => self.unify_const(x, y),
                _ => false,
            })
    }

    pub(crate) fn unify_const(&mut self, a: &Const, b: &Const) -> bool {
        let a = self.shallow_const(a);
        let b = self.shallow_const(b);
        match (&a, &b) {
            (Const::Error, _) | (_, Const::Error) => true,
            (Const::Infer(x), Const::Infer(y)) if x == y => true,
            (Const::Infer(var), other) | (other, Const::Infer(var)) => {
                self.set(*var, VarData::Const(Some(other.clone())));
                true
            }
            _ => a == b,
        }
    }

    /// Merges two unresolved type variables; literal hulls combine.
    fn union(&mut self, x: InferVar, y: InferVar) {
        match (self.literal_of(x), self.literal_of(y)) {
            (Some(lx), Some(ly)) => {
                self.set(
                    x,
                    VarData::Ty {
                        value: None,
                        literal: Some(Literal {
                            lo: lx.lo.min(ly.lo),
                            hi: lx.hi.max(ly.hi),
                            origin: lx.origin,
                        }),
                    },
                );
                self.set(
                    y,
                    VarData::Ty {
                        value: Some(Ty::Infer(x)),
                        literal: None,
                    },
                );
            }
            (Some(_), None) => self.set(
                y,
                VarData::Ty {
                    value: Some(Ty::Infer(x)),
                    literal: None,
                },
            ),
            _ => self.set(
                x,
                VarData::Ty {
                    value: Some(Ty::Infer(y)),
                    literal: None,
                },
            ),
        }
    }

    fn bind(&mut self, var: InferVar, ty: &Ty) -> bool {
        if self.occurs(var, ty) {
            return false;
        }
        if let Some(literal) = self.literal_of(var) {
            if !matches!(ty, Ty::Int(_) | Ty::Bounded { .. }) {
                return false;
            }
            self.fits.push(Fit {
                literal,
                ty: ty.clone(),
            });
        }
        self.set(
            var,
            VarData::Ty {
                value: Some(ty.clone()),
                literal: None,
            },
        );
        true
    }

    fn occurs(&self, var: InferVar, ty: &Ty) -> bool {
        let resolved = self.resolve(ty);
        let mut found = false;
        resolved.walk(&mut |node| {
            found |= matches!(node, TyNode::Ty(Ty::Infer(other)) if *other == var);
        });
        found
    }

    /// Every unresolved integer literal variable, in creation order.
    pub(crate) fn unresolved_literals(&self) -> Vec<(InferVar, Literal)> {
        self.vars
            .iter()
            .enumerate()
            .filter_map(|(index, data)| match data {
                VarData::Ty {
                    value: None,
                    literal: Some(literal),
                } => Some((InferVar(u32::try_from(index).ok()?), *literal)),
                _ => None,
            })
            .collect()
    }
}
