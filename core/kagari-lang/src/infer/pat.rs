//! Patterns: the type each pattern matches and the types of the locals it binds.

use super::ctx::InferCtx;
use crate::def_map::{Def, Resolution};
use crate::diagnostic::DiagnosticCode;
use crate::hir::{Literal, Pat, PatId, Path};
use crate::item_tree::{ItemKind, Shape, item_tree};
use crate::signatures::ItemSig;
use crate::signatures::subst::{Subst, subst_ty};
use crate::ty::{CtorRef, Ty};

impl InferCtx<'_> {
    /// Checks `pat` against the type of the matched value.
    pub(crate) fn check_pat(&mut self, pat: PatId, expected: &Ty) {
        self.check_pat_mode(pat, expected, None);
    }

    /// `by_ref`: the pattern matches through references (default binding mode), so bindings
    /// bind `&T` (`Some(false)`) or `&mut T` (`Some(true)`).
    fn check_pat_mode(&mut self, pat: PatId, expected: &Ty, by_ref: Option<bool>) {
        let store = self.store;
        let ty = match store.pat(pat) {
            Pat::Missing | Pat::Wild | Pat::Rest => expected.clone(),
            Pat::Bind { .. } => {
                if let Some(resolution) = self.res.store.pats.get(&pat).cloned() {
                    self.check_path_pat(pat, &resolution, None, expected, by_ref)
                } else {
                    if let Some(&local) = self.res.bindings.get(&pat) {
                        let local_ty = match by_ref {
                            Some(mutable) => Ty::Ref {
                                mutable,
                                inner: Box::new(expected.clone()),
                            },
                            None => expected.clone(),
                        };
                        self.result.local_types.insert(local, local_ty);
                    }
                    expected.clone()
                }
            }
            Pat::Path(path) => match self.res.store.pats.get(&pat).cloned() {
                Some(resolution) => {
                    self.check_path_pat(pat, &resolution, Some(path), expected, by_ref)
                }
                None => Ty::Error,
            },
            Pat::Lit { lit, negated } => {
                let (expected, _) = self.peel_refs(expected, by_ref);
                self.check_lit_pat(pat, lit, *negated, &expected);
                expected
            }
            Pat::Range { start, end, .. } => {
                let (expected, _) = self.peel_refs(expected, by_ref);
                for bound in [start, end].into_iter().flatten() {
                    if let crate::hir::PatBound::Lit {
                        lit: Literal::Int { value, .. },
                        negated,
                    } = bound
                    {
                        self.pat_int_fit(pat, *value, *negated, &expected);
                    }
                }
                let resolved = self.structurally_resolve(&expected);
                let is_integer = matches!(
                    resolved,
                    Ty::Int(_) | Ty::Bounded { .. } | Ty::Error | Ty::Infer(_)
                );
                if !is_integer {
                    let range = self.pat_range(pat);
                    let message = format!(
                        "range patterns match integers, but the value has type `{}`",
                        self.show(&expected)
                    );
                    self.diag(range, DiagnosticCode::TypeMismatch, message);
                }
                expected
            }
            Pat::Tuple(pats) => {
                let (expected, by_ref) = self.peel_refs(expected, by_ref);
                let resolved = self.structurally_resolve(&expected);
                let has_rest = pats.iter().any(|&sub| matches!(store.pat(sub), Pat::Rest));
                let fields = match resolved {
                    Ty::Tuple(fields) => Some(fields),
                    Ty::Infer(_) if !has_rest => {
                        let fields: Vec<Ty> = pats.iter().map(|_| self.table.new_var()).collect();
                        self.table.unify(&expected, &Ty::Tuple(fields.clone()));
                        Some(fields)
                    }
                    Ty::Error => None,
                    _ => {
                        let range = self.pat_range(pat);
                        let found = Ty::Tuple(pats.iter().map(|_| self.table.new_var()).collect());
                        self.mismatch(range, &expected, &found);
                        None
                    }
                };
                match fields {
                    Some(fields) => self.check_sub_pats(pat, pats, &fields, by_ref, "tuple"),
                    None => {
                        for &sub in pats {
                            self.check_pat_mode(sub, &Ty::Error, None);
                        }
                    }
                }
                expected
            }
            Pat::TupleStruct { path, pats } => {
                let (expected, by_ref) = self.peel_refs(expected, by_ref);
                let resolution = self.res.store.pats.get(&pat).cloned();
                let range = self.pat_range(pat);
                let ctor = match &resolution {
                    Some(Resolution::Def(Def::Variant { .. }))
                    | Some(Resolution::Def(Def::Item(_)))
                    | Some(Resolution::SelfType(_)) => {
                        self.ctor_of(resolution.as_ref(), path, range)
                    }
                    Some(Resolution::Error) | None => None,
                    Some(_) => {
                        self.diag(
                            range,
                            DiagnosticCode::TypeBadPattern,
                            "expected a tuple struct or tuple variant",
                        );
                        None
                    }
                };
                match ctor.and_then(|ctor| self.ctor_fields(pat, ctor, &expected, Shape::Tuple(0)))
                {
                    Some(fields) => {
                        let tys: Vec<Ty> = fields.into_iter().map(|(_, ty)| ty).collect();
                        self.check_sub_pats(pat, pats, &tys, by_ref, "tuple variant");
                    }
                    None => {
                        for &sub in pats {
                            self.check_pat_mode(sub, &Ty::Error, None);
                        }
                    }
                }
                expected
            }
            Pat::Record { path, fields } => {
                let (expected, by_ref) = self.peel_refs(expected, by_ref);
                let resolution = self.res.store.pats.get(&pat).cloned();
                let range = self.pat_range(pat);
                let ctor = self.ctor_of(resolution.as_ref(), path, range);
                let defs = ctor.and_then(|ctor| {
                    self.ctor_fields(pat, ctor, &expected, Shape::Record(Vec::new()))
                });
                for (name, sub) in fields {
                    let field_ty = match &defs {
                        Some(defs) => {
                            match defs.iter().find(|(field, _)| field.as_ref() == Some(name)) {
                                Some((_, ty)) => ty.clone(),
                                None => {
                                    let sub_range = self.pat_range(*sub);
                                    self.diag(
                                        sub_range,
                                        DiagnosticCode::TypeUnknownField,
                                        format!("no field named `{name}` in this pattern's type"),
                                    );
                                    Ty::Error
                                }
                            }
                        }
                        None => Ty::Error,
                    };
                    self.check_pat_mode(*sub, &field_ty, by_ref);
                }
                expected
            }
            Pat::Or(alternatives) => {
                for &alternative in alternatives {
                    self.check_pat_mode(alternative, expected, by_ref);
                }
                expected.clone()
            }
        };
        self.record_pat(pat, &ty);
    }

    /// Strips references off the matched type for a non-binding pattern.
    fn peel_refs(&mut self, expected: &Ty, mut by_ref: Option<bool>) -> (Ty, Option<bool>) {
        let mut current = expected.clone();
        loop {
            match self.structurally_resolve(&current) {
                Ty::Ref { mutable, inner } => {
                    by_ref = Some(by_ref.map_or(mutable, |outer| outer && mutable));
                    current = *inner;
                }
                _ => return (current, by_ref),
            }
        }
    }

    fn check_sub_pats(
        &mut self,
        pat: PatId,
        pats: &[PatId],
        fields: &[Ty],
        by_ref: Option<bool>,
        what: &str,
    ) {
        let store = self.store;
        let rest = pats
            .iter()
            .position(|&sub| matches!(store.pat(sub), Pat::Rest));
        let (before, after) = match rest {
            Some(rest) => (rest, pats.len().saturating_sub(rest).saturating_sub(1)),
            None => (pats.len(), 0),
        };
        let fits = match rest {
            Some(_) => before.saturating_add(after) <= fields.len(),
            None => pats.len() == fields.len(),
        };
        if !fits {
            let range = self.pat_range(pat);
            self.diag(
                range,
                DiagnosticCode::TypeBadPattern,
                format!(
                    "this pattern has {} fields, but the {what} has {}",
                    pats.len(),
                    fields.len()
                ),
            );
        }
        for (index, &sub) in pats.iter().enumerate() {
            let field = match rest {
                Some(rest) if index == rest => {
                    self.check_pat_mode(sub, &Ty::Error, None);
                    continue;
                }
                Some(rest) if index > rest => {
                    let from_end = pats.len().saturating_sub(index);
                    fields
                        .len()
                        .checked_sub(from_end)
                        .and_then(|index| fields.get(index))
                }
                _ => fields.get(index),
            };
            let field = field.cloned().unwrap_or(Ty::Error);
            self.check_pat_mode(sub, &field, by_ref);
        }
    }

    /// Unifies the constructor's type with the matched type and returns its fields, checking
    /// that the constructor has the shape the pattern needs.
    fn ctor_fields(
        &mut self,
        pat: PatId,
        ctor: CtorRef,
        expected: &Ty,
        shape: Shape,
    ) -> Option<Vec<(Option<crate::hir::Name>, Ty)>> {
        let range = self.pat_range(pat);
        let ty = Ty::Adt {
            item: ctor.adt,
            args: ctor.args.clone(),
        };
        if !self.try_unify(&ty, expected) {
            self.mismatch(range, expected, &ty);
            return None;
        }
        let def = self.sigs.adt(ctor.adt)?;
        if def.is_enum && ctor.variant.is_none() {
            self.diag(
                range,
                DiagnosticCode::TypeBadPattern,
                "expected a struct or enum variant, found an enum",
            );
            return None;
        }
        let variant = def.variants.get(ctor.variant.unwrap_or(0) as usize)?;
        let shape_ok = matches!(
            (&variant.shape, &shape),
            (Shape::Tuple(_), Shape::Tuple(_)) | (Shape::Record(_), Shape::Record(_))
        ) || (matches!(shape, Shape::Record(_))
            && matches!(variant.shape, Shape::Unit));
        if !shape_ok {
            let name = variant.name.clone();
            let message = match variant.shape {
                Shape::Unit => format!("`{name}` is a unit variant; match it without fields"),
                Shape::Tuple(_) => {
                    format!("`{name}` is a tuple variant; match it with `{name}(…)`")
                }
                Shape::Record(_) => {
                    format!("`{name}` is a struct variant; match it with `{name} {{ … }}`")
                }
            };
            self.diag(range, DiagnosticCode::TypeBadPattern, message);
            return None;
        }
        let subst = Subst {
            owners: &[(ctor.adt, 0)],
            args: &ctor.args,
            self_ty: None,
        };
        let fields = variant
            .fields
            .iter()
            .map(|field| (field.name.clone(), subst_ty(&field.ty, &subst)))
            .collect();
        self.result.pat_ctors.insert(pat, ctor);
        Some(fields)
    }

    /// An identifier or path pattern naming a unit struct, unit variant or constant.
    fn check_path_pat(
        &mut self,
        pat: PatId,
        resolution: &Resolution,
        path: Option<&Path>,
        expected: &Ty,
        by_ref: Option<bool>,
    ) -> Ty {
        let range = self.pat_range(pat);
        let (expected, _) = self.peel_refs(expected, by_ref);
        match resolution {
            Resolution::Def(Def::Item(item)) => {
                let kind = item_tree(self.db, item.file)
                    .item(item.index)
                    .map(|data| data.kind.clone());
                match kind {
                    Some(ItemKind::Const { .. }) => {
                        let ty = match self.sigs.get(*item) {
                            Some(ItemSig::Const { ty, .. }) => ty.clone(),
                            _ => Ty::Error,
                        };
                        if !self.try_unify(&ty, &expected) {
                            self.mismatch(range, &expected, &ty);
                        }
                        expected
                    }
                    Some(ItemKind::Static { .. }) => {
                        self.diag(
                            range,
                            DiagnosticCode::TypeBadPattern,
                            "statics cannot be used in patterns",
                        );
                        expected
                    }
                    _ => self.unit_ctor_pat(pat, resolution, path, &expected),
                }
            }
            Resolution::Def(Def::Variant { .. }) | Resolution::SelfType(_) => {
                self.unit_ctor_pat(pat, resolution, path, &expected)
            }
            Resolution::Assoc(item) => {
                let ty = match self.sigs.get(*item) {
                    Some(ItemSig::Const { ty, .. }) => ty.clone(),
                    _ => Ty::Error,
                };
                if !self.try_unify(&ty, &expected) {
                    self.mismatch(range, &expected, &ty);
                }
                expected
            }
            Resolution::Error => expected,
            _ => {
                self.diag(
                    range,
                    DiagnosticCode::TypeBadPattern,
                    "expected a unit struct, unit variant or constant",
                );
                expected
            }
        }
    }

    fn unit_ctor_pat(
        &mut self,
        pat: PatId,
        resolution: &Resolution,
        path: Option<&Path>,
        expected: &Ty,
    ) -> Ty {
        let range = self.pat_range(pat);
        let empty = Path {
            kind: crate::hir::PathKind::Plain,
            segments: Vec::new(),
        };
        let path = path.unwrap_or(&empty);
        let Some(ctor) = self.ctor_of(Some(resolution), path, range) else {
            return expected.clone();
        };
        let ty = Ty::Adt {
            item: ctor.adt,
            args: ctor.args.clone(),
        };
        if !self.try_unify(&ty, expected) {
            self.mismatch(range, expected, &ty);
            return expected.clone();
        }
        if let Some(def) = self.sigs.adt(ctor.adt)
            && let Some(variant) = def.variants.get(ctor.variant.unwrap_or(0) as usize)
            && variant.shape != Shape::Unit
        {
            let name = variant.name.clone();
            let message = match variant.shape {
                Shape::Tuple(_) => {
                    format!("`{name}` is a tuple variant; match it with `{name}(…)`")
                }
                _ => format!("`{name}` is a struct variant; match it with `{name} {{ … }}`"),
            };
            self.diag(range, DiagnosticCode::TypeBadPattern, message);
            return expected.clone();
        }
        self.result.pat_ctors.insert(pat, ctor);
        expected.clone()
    }

    /// An integer literal of a pattern must fit in the matched type; checked once the type
    /// is known.
    fn pat_int_fit(&mut self, pat: PatId, value: u128, negated: bool, expected: &Ty) {
        let range = self.pat_range(pat);
        let value = i128::try_from(value).ok().and_then(|value| {
            if negated {
                value.checked_neg()
            } else {
                Some(value)
            }
        });
        match value {
            Some(value) => self.pat_fits.push(super::ctx::PatFit {
                value,
                ty: expected.clone(),
                range,
            }),
            None => self.diag(
                range,
                DiagnosticCode::TypeLiteralOutOfRange,
                "integer literal is too large",
            ),
        }
    }

    fn check_lit_pat(&mut self, pat: PatId, lit: &Literal, negated: bool, expected: &Ty) {
        let range = self.pat_range(pat);
        let found = match lit {
            Literal::Int { value, .. } => {
                let resolved = self.structurally_resolve(expected);
                match resolved {
                    Ty::Error => {}
                    Ty::Int(_) | Ty::Bounded { .. } | Ty::Infer(_) => {
                        self.pat_int_fit(pat, *value, negated, expected);
                    }
                    _ => {
                        let message = format!(
                            "mismatched types: expected `{}`, found an integer",
                            self.show(expected)
                        );
                        self.diag(range, DiagnosticCode::TypeMismatch, message);
                    }
                }
                return;
            }
            Literal::Bool(_) => Ty::Bool,
            Literal::String(_) => self.std_adt(self.sigs.std.text),
            Literal::Decimal(_) => self.std_adt(self.sigs.std.decimal),
        };
        if !self.try_unify(&found, expected) {
            self.mismatch(range, expected, &found);
        }
    }
}
