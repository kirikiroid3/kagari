//! The form rules: `choice!` ids and arms, mono ruby readings and constant text effects;
//! and unused `#[must_use]` values.

use rustc_hash::FxHashMap;

use crate::def_map::{TypeKey, def_maps};
use crate::diagnostic::DiagnosticCode;
use crate::hir::{ArrayExpr, Expr, ExprId, Literal, Name, PatId};
use crate::infer::ValueRef;
use crate::item_tree::{ItemId, Shape};
use crate::resolve::LocalId;
use crate::signatures::{ItemSig, assoc_parent};
use crate::ty::Ty;

use super::body::{BodyChecker, MustUseLocal};

impl BodyChecker<'_, '_> {
    /// A `Choice { … }` record, written by `choice!` or by hand.
    pub(super) fn record(&mut self, expr: ExprId, fields: &[(Name, ExprId)]) {
        let Some(ctor) = self.infer.records.get(&expr) else {
            return;
        };
        if Some(ctor.adt) != self.env.sigs.std.choice {
            return;
        }
        let field = |name: &str| {
            fields
                .iter()
                .find(|(field, _)| field == name)
                .map(|(_, value)| *value)
        };
        if let Some(id) = field("id") {
            self.choice_id(id);
        }
        if let Some(arms) = field("arms") {
            self.choice_arms(arms);
        }
    }

    /// A string literal, unique among the choices of the function.
    fn choice_id(&mut self, id: ExprId) {
        let range = self.range(id);
        match self.store.expr(id) {
            Expr::Literal(Literal::String(text)) => {
                if let Some(&first) = self.choice_ids.get(text) {
                    let diag = self
                        .diag(
                            range,
                            DiagnosticCode::KindChoiceId,
                            format!(
                                "the choice id `{text}` is used twice in `{}`: a choice id must \
                                 be unique within its function, since saves record it",
                                self.env.fn_path(self.item)
                            ),
                        )
                        .with_related(self.path.clone(), first, "first used here");
                    self.report(diag);
                } else {
                    self.choice_ids.insert(text.clone(), range);
                }
            }
            Expr::Missing => {}
            _ if self.ty(id).is_none() => {}
            _ => {
                let diag = self.diag(
                    range,
                    DiagnosticCode::KindChoiceId,
                    "the `id` of a choice must be a string literal",
                );
                self.report(diag);
            }
        }
    }

    /// An array literal of `ChoiceArm { … }` records whose values are distinct fieldless
    /// variants of one enum.
    fn choice_arms(&mut self, arms: ExprId) {
        let store = self.store;
        let items = match store.expr(arms) {
            Expr::Array(ArrayExpr::List(items)) => items,
            Expr::Missing => return,
            _ if self.ty(arms).is_none() => return,
            _ => {
                let diag = self.diag(
                    self.range(arms),
                    DiagnosticCode::KindChoiceArms,
                    "the `arms` of a choice must be an array literal of `ChoiceArm { … }` records",
                );
                self.report(diag);
                return;
            }
        };
        let mut first: Option<ItemId> = None;
        let mut seen: FxHashMap<u32, ExprId> = FxHashMap::default();
        for &arm in items {
            let value = match store.expr(arm) {
                Expr::Record { fields, .. }
                    if self
                        .infer
                        .records
                        .get(&arm)
                        .is_some_and(|ctor| Some(ctor.adt) == self.env.sigs.std.choice_arm) =>
                {
                    fields
                        .iter()
                        .find(|(name, _)| name == "value")
                        .map(|(_, value)| *value)
                }
                Expr::Missing => continue,
                _ if self.ty(arm).is_none() => continue,
                _ => {
                    let diag = self.diag(
                        self.range(arm),
                        DiagnosticCode::KindChoiceArms,
                        "an arm of a choice must be a `ChoiceArm { … }` record",
                    );
                    self.report(diag);
                    continue;
                }
            };
            if let Some(value) = value {
                self.choice_arm_value(value, &mut first, &mut seen);
            }
        }
    }

    fn choice_arm_value(
        &mut self,
        value: ExprId,
        first: &mut Option<ItemId>,
        seen: &mut FxHashMap<u32, ExprId>,
    ) {
        if matches!(self.store.expr(value), Expr::Missing) || self.ty(value).is_none() {
            return;
        }
        let range = self.range(value);
        let variant = match (self.store.expr(value), self.infer.paths.get(&value)) {
            (Expr::Path(_), Some(ValueRef::Ctor(ctor))) => ctor.variant.and_then(|variant| {
                let adt = self.env.sigs.adt(ctor.adt)?;
                let data = adt.variants.get(variant as usize)?;
                (adt.is_enum && data.shape == Shape::Unit).then_some((ctor.adt, variant))
            }),
            _ => None,
        };
        let Some((adt, variant)) = variant else {
            let diag = self.diag(
                range,
                DiagnosticCode::KindChoiceArms,
                "the value of a choice arm must be a fieldless enum variant, such as `Answer::Yes`",
            );
            self.report(diag);
            return;
        };
        match *first {
            // Arms of another enum are a type mismatch, which inference reports.
            Some(first_adt) if first_adt != adt => return,
            Some(_) => {}
            None => *first = Some(adt),
        }
        if let Some(&previous) = seen.get(&variant) {
            let diag = self
                .diag(
                    range,
                    DiagnosticCode::KindChoiceArms,
                    format!(
                        "`{}` is already an arm of this choice: each variant may be chosen at \
                         most once",
                        self.src(value)
                    ),
                )
                .with_related(self.path.clone(), self.range(previous), "first used here");
            self.report(diag);
        } else {
            seen.insert(variant, value);
        }
    }

    /// `mono_ruby(base, [readings…])`: one reading per character of the base literal.
    pub(super) fn mono_ruby(&mut self, expr: ExprId, args: &[ExprId]) {
        let [base, readings] = args else {
            return;
        };
        let text = match self.store.expr(*base) {
            Expr::Literal(Literal::String(text)) => text,
            Expr::Missing => return,
            _ if self.ty(*base).is_none() => return,
            _ => {
                let diag = self.diag(
                    self.range(*base),
                    DiagnosticCode::KindRubyReadings,
                    "the base text of a ruby must be a string literal",
                );
                self.report(diag);
                return;
            }
        };
        let count = match self.store.expr(*readings) {
            Expr::Array(ArrayExpr::List(items)) => items.len(),
            Expr::Missing => return,
            _ if self.ty(*readings).is_none() => return,
            _ => {
                let diag = self.diag(
                    self.range(*readings),
                    DiagnosticCode::KindRubyReadings,
                    "the readings of a mono ruby must be an array literal",
                );
                self.report(diag);
                return;
            }
        };
        let chars = text.chars().count();
        if chars == count {
            return;
        }
        let diag = self.diag(
            self.range(expr),
            DiagnosticCode::KindRubyReadings,
            format!(
                "a mono ruby needs one reading per character: `{text}` has {} but {} given",
                plural(chars, "character"),
                were(count, "reading")
            ),
        );
        self.report(diag);
    }

    /// `effect(base, e)`: the effect is a constant expression. Returns the effect argument
    /// when it is reported.
    pub(super) fn effect(&mut self, args: &[ExprId]) -> Option<ExprId> {
        let &effect = args.get(1)?;
        if matches!(self.store.expr(effect), Expr::Missing)
            || self.ty(effect).is_none()
            || self.consts.contains(effect)
        {
            return None;
        }
        let diag = self.diag(
            self.range(effect),
            DiagnosticCode::KindEffectNotConstant,
            "the effect of a text effect must be a constant expression",
        );
        self.report(diag);
        Some(effect)
    }

    // ---- `#[must_use]` ----

    /// The `#[must_use]` ADT of `ty` or of the value it references.
    fn must_use_adt(&self, mut ty: &Ty) -> Option<ItemId> {
        while let Ty::Ref { inner, .. } = ty {
            ty = inner;
        }
        match ty {
            Ty::Adt { item, .. } if self.env.sigs.adt(*item).is_some_and(|adt| adt.must_use) => {
                Some(*item)
            }
            _ => None,
        }
    }

    /// "unused `Swap` that must be used: the change does not happen without
    /// `apply_animation`". The methods of the type itself complete it.
    fn unused_message(&self, ty: &Ty, adt: ItemId) -> String {
        let mut inner = ty;
        while let Ty::Ref { inner: next, .. } = inner {
            inner = next;
        }
        let mut message = format!("unused `{}` that must be used", self.env.show(inner));
        let maps = def_maps(self.env.db, self.env.program);
        let mut methods: Vec<&Name> = maps
            .inherent
            .get(&TypeKey::Item(adt))
            .map(|methods| methods.keys().collect())
            .unwrap_or_default();
        methods.sort();
        if !methods.is_empty() {
            let list = methods
                .iter()
                .map(|name| format!("`{name}`"))
                .collect::<Vec<_>>()
                .join(" or ");
            if self.env.is_std(adt) {
                message.push_str(&format!(": the change does not happen without {list}"));
            } else {
                message.push_str(&format!(": complete it with {list}"));
            }
        }
        message
    }

    /// An expression statement whose value is `#[must_use]`.
    pub(super) fn must_use_stmt(&mut self, expr: ExprId) {
        let Some(ty) = self.ty(expr) else {
            return;
        };
        let Some(adt) = self.must_use_adt(ty) else {
            return;
        };
        let message = self.unused_message(ty, adt);
        let diag = self.diag(self.range(expr), DiagnosticCode::KindUnusedMustUse, message);
        self.report(diag);
    }

    /// A `let` binding a `#[must_use]` value to `local`.
    pub(super) fn register_must_use(&mut self, local: LocalId, pat: PatId) {
        let Some(ty) = self.infer.local_types.get(local) else {
            return;
        };
        if ty.references_error() {
            return;
        }
        if let Some(adt) = self.must_use_adt(ty) {
            self.must_use.push(MustUseLocal {
                local,
                pat,
                adt,
                ty: ty.clone(),
                used: false,
            });
        }
    }

    /// A method call on `receiver`: completes a `#[must_use]` local when the method belongs
    /// to the local's type.
    pub(super) fn must_use_receiver(&mut self, receiver: ExprId, method: ItemId) {
        let Some(ValueRef::Local(local)) = self.infer.paths.get(&receiver) else {
            return;
        };
        let Some(entry) = self.must_use.iter_mut().find(|entry| entry.local == *local) else {
            return;
        };
        let owner =
            assoc_parent(self.env.db, method).and_then(|parent| match self.env.sigs.get(parent) {
                Some(ItemSig::Impl {
                    self_ty: Ty::Adt { item, .. },
                    ..
                }) => Some(*item),
                _ => None,
            });
        if owner == Some(entry.adt) {
            entry.used = true;
        }
    }

    /// Warns about `#[must_use]` locals never completed.
    pub(super) fn finish_must_use(&mut self) {
        let unused: Vec<(PatId, Ty, ItemId)> = self
            .must_use
            .iter()
            .filter(|entry| !entry.used)
            .map(|entry| (entry.pat, entry.ty.clone(), entry.adt))
            .collect();
        for (pat, ty, adt) in unused {
            let message = self.unused_message(&ty, adt);
            let diag = self.diag(
                self.store.pat_range(pat),
                DiagnosticCode::KindUnusedMustUse,
                message,
            );
            self.report(diag);
        }
    }
}

fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

/// "1 reading is" / "3 readings are".
fn were(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun} is")
    } else {
        format!("{count} {noun}s are")
    }
}
