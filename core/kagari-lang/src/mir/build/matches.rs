//! Patterns, compiled to decision trees with Jacobs' algorithm.
//!
//! A row is the list of tests a candidate still has to pass (a place and the refutable
//! pattern it must match), the bindings it collected and the arm it leads to. Irrefutable
//! parts are simplified away: wildcards vanish, bindings move to the row's bindings, tuples
//! and structs become tests on their fields, and or-patterns become one row per
//! alternative. Then the first test of the first row is branched on: an enum variant with
//! `SwitchVariant` (its fields read through `Downcast`), a literal, constant or range with
//! comparisons. Every row is specialized to each branch; when the first row has no test
//! left, it matched: its bindings are bound and its guard tested, falling back to the
//! remaining rows when the guard is false.

use smallvec::SmallVec;

use crate::hir::BinaryOp;
use crate::mir::{ConstOperand, Operand, Place, Proj, Rvalue, Terminator};
use crate::resolve::LocalId;
use crate::thir::{RangeBound, ThirArm, ThirExprId, ThirPatId, ThirPatKind};
use crate::ty::GenericArgs;

use super::{BasicBlock, BlockAnd, Builder};

#[derive(Clone)]
struct Test {
    /// The place matched, with the pattern's references followed.
    place: Place,
    pat: ThirPatId,
}

#[derive(Clone)]
struct Row {
    tests: Vec<Test>,
    /// `(local, place, by_ref)`.
    bindings: Vec<(LocalId, Place, Option<bool>)>,
    arm: usize,
}

/// Where a matching candidate goes.
#[derive(Clone, Copy)]
pub(super) struct ArmTarget {
    pub(super) guard: Option<ThirExprId>,
    pub(super) target: BasicBlock,
}

impl Builder<'_> {
    pub(super) fn match_expr(
        &mut self,
        dest: Option<Place>,
        mut block: BasicBlock,
        scrutinee: ThirExprId,
        arms: &[ThirArm],
    ) -> BlockAnd<()> {
        // A guard that writes could change a scrutinee read in place: copy it first.
        let guards_pure = arms
            .iter()
            .filter_map(|arm| arm.guard)
            .all(|guard| self.is_pure(guard));
        let place = if guards_pure {
            unpack!(block = self.as_place(block, scrutinee))
        } else {
            Place::local(unpack!(block = self.as_temp(block, scrutinee)))
        };
        let targets: Vec<ArmTarget> = arms
            .iter()
            .map(|arm| ArmTarget {
                guard: arm.guard,
                target: self.new_block(),
            })
            .collect();
        let pats: Vec<ThirPatId> = arms.iter().map(|arm| arm.pat).collect();
        let otherwise = self.new_block();
        self.terminate(otherwise, Terminator::Unreachable);
        self.decision_tree(block, &place, &pats, &targets, otherwise);
        let join = self.new_block();
        for (arm, target) in arms.iter().zip(&targets) {
            let BlockAnd(end, ()) = self.expr_into_dest(dest.clone(), target.target, arm.body);
            self.goto(end, join);
        }
        BlockAnd(join, ())
    }

    /// Matches `place` against `pat`: to `then` with the bindings bound, else to `other`.
    pub(super) fn test_let(
        &mut self,
        block: BasicBlock,
        place: Place,
        pat: ThirPatId,
        then: BasicBlock,
        other: BasicBlock,
    ) {
        let target = ArmTarget {
            guard: None,
            target: then,
        };
        self.decision_tree(block, &place, &[pat], &[target], other);
    }

    /// Binds the irrefutable `pat` to `place`; returns the block after the bindings.
    pub(super) fn bind_irrefutable(
        &mut self,
        block: BasicBlock,
        place: Place,
        pat: ThirPatId,
    ) -> BasicBlock {
        let then = self.new_block();
        let other = self.new_block();
        self.terminate(other, Terminator::Unreachable);
        self.test_let(block, place, pat, then, other);
        then
    }

    /// Matches `place` against `pats[i]` for arm `targets[i]`, first match wins; to
    /// `otherwise` when none matches.
    fn decision_tree(
        &mut self,
        block: BasicBlock,
        place: &Place,
        pats: &[ThirPatId],
        targets: &[ArmTarget],
        otherwise: BasicBlock,
    ) {
        let mut rows = Vec::with_capacity(pats.len());
        for (arm, &pat) in pats.iter().enumerate() {
            let row = Row {
                tests: vec![self.test(place.clone(), pat)],
                bindings: Vec::new(),
                arm,
            };
            self.simplify(row, &mut rows);
        }
        self.compile(block, rows, targets, otherwise);
    }

    /// The test of `pat` on `place`, after the references the pattern follows.
    fn test(&self, mut place: Place, pat: ThirPatId) -> Test {
        for _ in 0..self.thir.pat(pat).derefs {
            place = place.project(Proj::Deref);
        }
        Test { place, pat }
    }

    /// Simplifies `row` until all its tests are refutable, appending the resulting rows
    /// (several for or-patterns) to `out` in order.
    fn simplify(&self, mut row: Row, out: &mut Vec<Row>) {
        stacker::maybe_grow(crate::STACK_RED_ZONE, crate::STACK_GROWTH, || {
            let thir = self.thir;
            let mut index = 0;
            while let Some(test) = row.tests.get(index).cloned() {
                match &thir.pat(test.pat).kind {
                    ThirPatKind::Wild => {
                        row.tests.remove(index);
                    }
                    ThirPatKind::Binding { local, by_ref } => {
                        row.bindings.push((*local, test.place, *by_ref));
                        row.tests.remove(index);
                    }
                    ThirPatKind::Tuple(subs) => {
                        let fields = subs.iter().enumerate().filter_map(|(field, sub)| {
                            let field = u32::try_from(field).ok()?;
                            Some(self.test(test.place.clone().project(Proj::Field(field)), *sub))
                        });
                        row.tests.splice(index..=index, fields.collect::<Vec<_>>());
                    }
                    ThirPatKind::Adt {
                        variant: None,
                        fields,
                        ..
                    } => {
                        let fields: Vec<Test> = fields
                            .iter()
                            .map(|(field, sub)| {
                                self.test(test.place.clone().project(Proj::Field(*field)), *sub)
                            })
                            .collect();
                        row.tests.splice(index..=index, fields);
                    }
                    ThirPatKind::Or(alternatives) => {
                        for alternative in alternatives {
                            let mut alt = row.clone();
                            if let Some(slot) = alt.tests.get_mut(index) {
                                *slot = self.test(test.place.clone(), *alternative);
                            }
                            self.simplify(alt, out);
                        }
                        return;
                    }
                    ThirPatKind::Lit(_)
                    | ThirPatKind::Const(_)
                    | ThirPatKind::Range { .. }
                    | ThirPatKind::Adt {
                        variant: Some(_), ..
                    }
                    | ThirPatKind::Error => index = index.saturating_add(1),
                }
            }
            out.push(row);
        })
    }

    fn compile(
        &mut self,
        block: BasicBlock,
        rows: Vec<Row>,
        targets: &[ArmTarget],
        otherwise: BasicBlock,
    ) {
        stacker::maybe_grow(crate::STACK_RED_ZONE, crate::STACK_GROWTH, || {
            let Some(first) = rows.first() else {
                self.goto(block, otherwise);
                return;
            };
            let Some(test) = first.tests.first().cloned() else {
                self.leaf(block, rows, targets, otherwise);
                return;
            };
            match &self.thir.pat(test.pat).kind {
                ThirPatKind::Adt {
                    adt,
                    variant: Some(_),
                    ..
                } => self.switch_variant(block, &test.place, *adt, rows, targets, otherwise),
                ThirPatKind::Lit(_) | ThirPatKind::Const(_) | ThirPatKind::Range { .. } => {
                    self.switch_value(block, &test, rows, targets, otherwise);
                }
                _ => self.terminate(block, Terminator::Unreachable),
            }
        })
    }

    /// The first row matched: bind, test its guard, fall back to the other rows.
    fn leaf(
        &mut self,
        mut block: BasicBlock,
        mut rows: Vec<Row>,
        targets: &[ArmTarget],
        otherwise: BasicBlock,
    ) {
        let row = rows.remove(0);
        for (local, place, by_ref) in row.bindings {
            let local = self.local(local);
            let rvalue = match by_ref {
                Some(mutable) => Rvalue::Ref { mutable, place },
                None => Rvalue::Use(Operand::Copy(place)),
            };
            self.assign(block, Place::local(local), rvalue);
        }
        let Some(target) = targets.get(row.arm).copied() else {
            self.terminate(block, Terminator::Unreachable);
            return;
        };
        match target.guard {
            None => self.goto(block, target.target),
            Some(guard) => {
                let cond = unpack!(block = self.as_operand(block, guard));
                let fallback = self.new_block();
                self.with_src(guard, |this| {
                    this.switch(block, cond, target.target, fallback);
                });
                self.compile(fallback, rows, targets, otherwise);
            }
        }
    }

    /// Branches on the variant of the enum at `place`.
    fn switch_variant(
        &mut self,
        block: BasicBlock,
        place: &Place,
        adt: crate::item_tree::ItemId,
        rows: Vec<Row>,
        targets: &[ArmTarget],
        otherwise: BasicBlock,
    ) {
        let thir = self.thir;
        let variant_of = |row: &Row| {
            row.tests
                .iter()
                .position(|test| test.place == *place)
                .and_then(|index| {
                    let test = row.tests.get(index)?;
                    match &thir.pat(test.pat).kind {
                        ThirPatKind::Adt {
                            variant: Some(variant),
                            fields,
                            ..
                        } => Some((index, *variant, fields)),
                        _ => None,
                    }
                })
        };
        let mut variants: Vec<u32> = Vec::new();
        for row in &rows {
            if let Some((_, variant, _)) = variant_of(row)
                && !variants.contains(&variant)
            {
                variants.push(variant);
            }
        }
        let mut branches: SmallVec<[(u32, BasicBlock); 4]> = SmallVec::new();
        for &variant in &variants {
            let mut sub = Vec::new();
            for row in &rows {
                match variant_of(row) {
                    None => sub.push(row.clone()),
                    Some((_, other, _)) if other != variant => {}
                    Some((index, _, fields)) => {
                        let base = place.clone().project(Proj::Downcast(variant));
                        let fields: Vec<Test> = fields
                            .iter()
                            .map(|(field, pat)| {
                                self.test(base.clone().project(Proj::Field(*field)), *pat)
                            })
                            .collect();
                        let mut row = row.clone();
                        row.tests.splice(index..=index, fields);
                        self.simplify(row, &mut sub);
                    }
                }
            }
            let target = self.new_block();
            self.compile(target, sub, targets, otherwise);
            branches.push((variant, target));
        }
        let total = self
            .cx
            .sigs
            .adt(adt)
            .map_or(usize::MAX, |def| def.variants.len());
        let rest = if variants.len() < total {
            let rest: Vec<Row> = rows
                .into_iter()
                .filter(|row| variant_of(row).is_none())
                .collect();
            let target = self.new_block();
            self.compile(target, rest, targets, otherwise);
            Some(target)
        } else {
            None
        };
        self.terminate(
            block,
            Terminator::SwitchVariant {
                place: place.clone(),
                adt,
                targets: branches,
                otherwise: rest,
            },
        );
    }

    /// Branches on a literal, constant or range test of the first row.
    fn switch_value(
        &mut self,
        block: BasicBlock,
        test: &Test,
        rows: Vec<Row>,
        targets: &[ArmTarget],
        otherwise: BasicBlock,
    ) {
        let thir = self.thir;
        let kind = &thir.pat(test.pat).kind;
        let mut yes = Vec::new();
        let mut no = Vec::new();
        for row in rows {
            let found = row.tests.iter().position(|other| other.place == test.place);
            let Some(index) = found else {
                yes.push(row.clone());
                no.push(row);
                continue;
            };
            let other = row.tests.get(index).map(|other| &thir.pat(other.pat).kind);
            if other == Some(kind) {
                // The same test: passed on one side, failed on the other.
                let mut row = row;
                row.tests.remove(index);
                yes.push(row);
            } else if matches!(
                (kind, other),
                (ThirPatKind::Lit(_), Some(ThirPatKind::Lit(_)))
            ) {
                // A different literal cannot match when this one did.
                no.push(row);
            } else {
                yes.push(row.clone());
                no.push(row);
            }
        }
        let yes_block = self.new_block();
        let no_block = self.new_block();
        let value = Operand::Copy(test.place.clone());
        match kind {
            ThirPatKind::Lit(_) | ThirPatKind::Const(_) => {
                let constant = match kind {
                    ThirPatKind::Lit(value) => Self::const_operand(value.clone()),
                    ThirPatKind::Const(item) => Self::item_operand(*item),
                    _ => Self::const_operand(crate::eval::ConstValue::unit()),
                };
                let equal = self.compare(block, BinaryOp::Eq, value, constant);
                self.switch(block, equal, yes_block, no_block);
            }
            ThirPatKind::Range { lo, hi, inclusive } => {
                let mut current = block;
                if let Some(lo) = lo {
                    let next = if hi.is_some() {
                        self.new_block()
                    } else {
                        yes_block
                    };
                    let above = self.compare(current, BinaryOp::Ge, value.clone(), Self::bound(lo));
                    self.switch(current, above, next, no_block);
                    current = next;
                }
                match hi {
                    Some(hi) => {
                        let op = if *inclusive {
                            BinaryOp::Le
                        } else {
                            BinaryOp::Lt
                        };
                        let below = self.compare(current, op, value, Self::bound(hi));
                        self.switch(current, below, yes_block, no_block);
                    }
                    None if lo.is_none() => self.goto(current, yes_block),
                    None => {}
                }
            }
            _ => self.terminate(block, Terminator::Unreachable),
        }
        self.compile(yes_block, yes, targets, otherwise);
        self.compile(no_block, no, targets, otherwise);
    }

    fn item_operand(item: crate::item_tree::ItemId) -> Operand {
        Operand::Const(ConstOperand::Item {
            item,
            args: GenericArgs::empty(),
        })
    }

    fn bound(bound: &RangeBound) -> Operand {
        match bound {
            RangeBound::Value(value) => Self::const_operand(value.clone()),
            RangeBound::Const(item) => Self::item_operand(*item),
        }
    }
}
