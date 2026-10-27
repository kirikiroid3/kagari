//! Patterns, with the references each one follows made explicit (`ThirPat::derefs`).
//!
//! A pattern other than a binding, a wildcard or an or-pattern matches through references:
//! it follows every reference at its position before testing, and the bindings under it
//! then bind references to the matched places (inference's default binding mode).

use crate::def_map::{Def, Resolution};
use crate::hir::{Name, Pat, PatBound, PatId};
use crate::signatures::subst::{Subst, subst_ty};
use crate::thir::{RangeBound, ThirPat, ThirPatId, ThirPatKind, literal_value};
use crate::ty::{CtorRef, Ty, TyKind};

use super::Builder;

impl Builder<'_> {
    /// A top-level pattern matched against a value of type `expected` (`None`: the
    /// pattern's own type).
    pub(super) fn root_pat(&mut self, pat: PatId, expected: Option<&Ty>) -> ThirPatId {
        let expected = match expected {
            Some(ty) => ty.clone(),
            None => self.pat_ty(pat),
        };
        self.pat(pat, &expected, None)
    }

    fn pat_ty(&self, pat: PatId) -> Ty {
        self.infer
            .pat_types
            .get(pat)
            .cloned()
            .unwrap_or_else(Ty::error)
    }

    /// `pat` matched against a value of type `expected`; `by_ref` as in inference: the
    /// binding mode under the references followed so far.
    fn pat(&mut self, pat: PatId, expected: &Ty, by_ref: Option<bool>) -> ThirPatId {
        stacker::maybe_grow(crate::STACK_RED_ZONE, crate::STACK_GROWTH, || {
            self.pat_inner(pat, expected, by_ref)
        })
    }

    fn pat_inner(&mut self, pat: PatId, expected: &Ty, mode: Option<bool>) -> ThirPatId {
        let ty = self.pat_ty(pat);
        let body = self.body;
        let simple = |kind| ThirPat {
            kind,
            ty: ty.clone(),
            src: pat,
            derefs: 0,
        };
        // A testing pattern follows the references at its position; its sub-patterns
        // match in the resulting binding mode.
        let (peeled, derefs, by_ref) = peel(expected, mode);
        let tested = |kind| ThirPat {
            kind,
            ty: ty.clone(),
            src: pat,
            derefs,
        };
        let out = match body.store.pat(pat) {
            Pat::Missing => simple(ThirPatKind::Error),
            Pat::Wild | Pat::Rest => simple(ThirPatKind::Wild),
            Pat::Bind { .. } if !self.res.store.pats.contains_key(&pat) => {
                match self.res.bindings.get(&pat) {
                    Some(&local) => simple(ThirPatKind::Binding {
                        local,
                        by_ref: mode,
                    }),
                    None => simple(ThirPatKind::Error),
                }
            }
            Pat::Bind { .. } | Pat::Path(_) => tested(self.path_pat(pat)),
            Pat::Lit { lit, negated } => tested(match literal_value(lit, *negated) {
                Some(value) => ThirPatKind::Lit(value),
                None => ThirPatKind::Error,
            }),
            Pat::Range {
                start,
                end,
                inclusive,
            } => {
                let lo = start.as_ref().map(|bound| self.range_bound(pat, 0, bound));
                let hi = end.as_ref().map(|bound| self.range_bound(pat, 1, bound));
                tested(match (lo, hi) {
                    (Some(None), _) | (_, Some(None)) => ThirPatKind::Error,
                    (lo, hi) => ThirPatKind::Range {
                        lo: lo.flatten(),
                        hi: hi.flatten(),
                        inclusive: *inclusive,
                    },
                })
            }
            Pat::Tuple(pats) => {
                let fields: Vec<Ty> = match peeled.kind() {
                    TyKind::Tuple(fields) => fields.to_vec(),
                    _ => pats.iter().map(|sub| self.pat_ty(*sub)).collect(),
                };
                let subs = self.sub_pats(pats, &fields, by_ref);
                tested(ThirPatKind::Tuple(
                    subs.into_iter().map(|(_, sub)| sub).collect(),
                ))
            }
            Pat::TupleStruct { pats, .. } => {
                let kind = match self.infer.pat_ctors.get(&pat).cloned() {
                    Some(ctor) => {
                        let fields: Vec<Ty> = self
                            .ctor_fields(&ctor)
                            .into_iter()
                            .map(|(_, ty)| ty)
                            .collect();
                        let subs = self.sub_pats(pats, &fields, by_ref);
                        ThirPatKind::Adt {
                            adt: ctor.adt,
                            variant: ctor.variant,
                            fields: subs,
                        }
                    }
                    None => ThirPatKind::Error,
                };
                tested(kind)
            }
            Pat::Record { fields, .. } => tested(self.record_pat(pat, fields, by_ref)),
            Pat::Or(alternatives) => {
                // Each alternative follows the references itself.
                let subs = alternatives
                    .iter()
                    .map(|alternative| self.pat(*alternative, expected, mode))
                    .collect();
                simple(ThirPatKind::Or(subs))
            }
        };
        self.pat_id(out)
    }

    /// A path pattern (or an identifier that resolves to an item): a unit struct or
    /// variant, or a constant.
    fn path_pat(&self, pat: PatId) -> ThirPatKind {
        if let Some(ctor) = self.infer.pat_ctors.get(&pat) {
            return ThirPatKind::Adt {
                adt: ctor.adt,
                variant: ctor.variant,
                fields: Vec::new(),
            };
        }
        match self.res.store.pats.get(&pat) {
            Some(Resolution::Def(Def::Item(item)) | Resolution::Assoc(item)) => {
                ThirPatKind::Const(*item)
            }
            _ => ThirPatKind::Error,
        }
    }

    /// `None` when the bound cannot be evaluated.
    fn range_bound(&self, pat: PatId, which: u8, bound: &PatBound) -> Option<RangeBound> {
        match bound {
            PatBound::Lit { lit, negated } => literal_value(lit, *negated).map(RangeBound::Value),
            PatBound::Path(_) => match self.res.store.range_bounds.get(&(pat, which)) {
                Some(Resolution::Def(Def::Item(item)) | Resolution::Assoc(item)) => {
                    Some(RangeBound::Const(*item))
                }
                _ => None,
            },
        }
    }

    fn record_pat(
        &mut self,
        pat: PatId,
        fields: &[(Name, PatId)],
        by_ref: Option<bool>,
    ) -> ThirPatKind {
        let Some(ctor) = self.infer.pat_ctors.get(&pat).cloned() else {
            return ThirPatKind::Error;
        };
        let defs = self.ctor_fields(&ctor);
        let mut out = Vec::with_capacity(fields.len());
        for (name, sub) in fields {
            let Some((index, (_, ty))) = defs
                .iter()
                .enumerate()
                .find(|(_, (field, _))| field.as_ref() == Some(name))
            else {
                return ThirPatKind::Error;
            };
            let Ok(index) = u32::try_from(index) else {
                return ThirPatKind::Error;
            };
            let ty = ty.clone();
            out.push((index, self.pat(*sub, &ty, by_ref)));
        }
        ThirPatKind::Adt {
            adt: ctor.adt,
            variant: ctor.variant,
            fields: out,
        }
    }

    /// The fields of `ctor`'s variant with the ADT's generic arguments substituted.
    fn ctor_fields(&self, ctor: &CtorRef) -> Vec<(Option<Name>, Ty)> {
        let Some(variant) = self
            .sigs
            .adt(ctor.adt)
            .and_then(|adt| adt.variants.get(ctor.variant.unwrap_or(0) as usize))
        else {
            return Vec::new();
        };
        let subst = Subst {
            owners: &[(ctor.adt, 0)],
            args: &ctor.args,
            self_ty: None,
        };
        variant
            .fields
            .iter()
            .map(|field| (field.name.clone(), subst_ty(&field.ty, &subst)))
            .collect()
    }

    /// The sub-patterns of a tuple or tuple struct with at most one `..`, one per field:
    /// the fields `..` covers are wildcards.
    fn sub_pats(
        &mut self,
        pats: &[PatId],
        fields: &[Ty],
        by_ref: Option<bool>,
    ) -> Vec<(u32, ThirPatId)> {
        let body = self.body;
        let rest = pats
            .iter()
            .position(|sub| matches!(body.store.pat(*sub), Pat::Rest));
        // `(field index, HIR sub-pattern)`; `None` for a wildcard `..` covers.
        let mut slots: Vec<(usize, Option<PatId>)> = Vec::with_capacity(fields.len());
        let mut rest_src = None;
        match rest {
            None => slots.extend(pats.iter().map(|&sub| Some(sub)).enumerate()),
            Some(rest) => {
                rest_src = pats.get(rest).copied();
                let after = pats.len().saturating_sub(rest).saturating_sub(1);
                let skip = fields.len().saturating_sub(after).max(rest);
                slots.extend(pats.iter().take(rest).map(|&sub| Some(sub)).enumerate());
                slots.extend((rest..skip).map(|index| (index, None)));
                slots.extend(
                    pats.iter()
                        .skip(rest.saturating_add(1))
                        .enumerate()
                        .map(|(offset, &sub)| (skip.saturating_add(offset), Some(sub))),
                );
            }
        }
        let mut out = Vec::with_capacity(slots.len());
        for (index, sub) in slots {
            let ty = fields.get(index).cloned().unwrap_or_else(Ty::error);
            let id = match (sub, rest_src) {
                (Some(sub), _) => self.pat(sub, &ty, by_ref),
                (None, Some(src)) => self.pat_id(ThirPat {
                    kind: ThirPatKind::Wild,
                    ty,
                    src,
                    derefs: 0,
                }),
                (None, None) => continue,
            };
            if let Ok(index) = u32::try_from(index) {
                out.push((index, id));
            }
        }
        out
    }
}

/// Follows the references of `expected` as a non-binding pattern does: the type under
/// them, how many there are (saturating at `u8::MAX`), and the binding mode below.
fn peel(expected: &Ty, mut by_ref: Option<bool>) -> (Ty, u8, Option<bool>) {
    let mut current = expected.clone();
    let mut derefs = 0u8;
    while let TyKind::Ref { mutable, inner } = current.kind() {
        by_ref = Some(by_ref.map_or(*mutable, |outer| outer && *mutable));
        derefs = derefs.saturating_add(1);
        current = inner.clone();
    }
    (current, derefs, by_ref)
}
