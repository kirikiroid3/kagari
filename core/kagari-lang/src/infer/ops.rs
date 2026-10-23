//! Operators: `Int` range arithmetic, Rust integers, booleans, comparisons and assignment.

use kagari_syntax::TextRange;

use super::ctx::{Expect, InferCtx, Obligation, ObligationKind};
use crate::diagnostic::DiagnosticCode;
use crate::hir::{BinaryOp, ExprId};
use crate::ty::{IntTy, Ty};

fn op_text(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Rem => "%",
        BinaryOp::BitAnd => "&",
        BinaryOp::BitOr => "|",
        BinaryOp::BitXor => "^",
        BinaryOp::Shl => "<<",
        BinaryOp::Shr => ">>",
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Le => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::Ge => ">=",
        BinaryOp::And => "&&",
        BinaryOp::Or => "||",
    }
}

fn is_bitwise(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor | BinaryOp::Shl | BinaryOp::Shr
    )
}

fn is_shift(op: BinaryOp) -> bool {
    matches!(op, BinaryOp::Shl | BinaryOp::Shr)
}

pub(crate) enum RangeError {
    /// The divisor's range contains 0.
    Zero,
    Overflow,
}

/// The range of `a op b` for `a` in `lhs` and `b` in `rhs` (`+ - * / %`).
pub(crate) fn range_op(
    op: BinaryOp,
    (l1, h1): (i128, i128),
    (l2, h2): (i128, i128),
) -> Result<(i128, i128), RangeError> {
    let corners = |f: fn(i128, i128) -> Option<i128>| -> Result<(i128, i128), RangeError> {
        let values = [f(l1, l2), f(l1, h2), f(h1, l2), f(h1, h2)];
        let mut lo = i128::MAX;
        let mut hi = i128::MIN;
        for value in values {
            let value = value.ok_or(RangeError::Overflow)?;
            lo = lo.min(value);
            hi = hi.max(value);
        }
        Ok((lo, hi))
    };
    match op {
        BinaryOp::Add => Ok((
            l1.checked_add(l2).ok_or(RangeError::Overflow)?,
            h1.checked_add(h2).ok_or(RangeError::Overflow)?,
        )),
        BinaryOp::Sub => Ok((
            l1.checked_sub(h2).ok_or(RangeError::Overflow)?,
            h1.checked_sub(l2).ok_or(RangeError::Overflow)?,
        )),
        BinaryOp::Mul => corners(i128::checked_mul),
        BinaryOp::Div => {
            if l2 <= 0 && 0 <= h2 {
                return Err(RangeError::Zero);
            }
            corners(i128::checked_div)
        }
        BinaryOp::Rem => {
            if l2 <= 0 && 0 <= h2 {
                return Err(RangeError::Zero);
            }
            let magnitude = l2
                .checked_abs()
                .ok_or(RangeError::Overflow)?
                .max(h2.checked_abs().ok_or(RangeError::Overflow)?);
            let bound = magnitude.checked_sub(1).ok_or(RangeError::Overflow)?;
            let lo = if l1 >= 0 {
                0
            } else {
                l1.max(bound.checked_neg().ok_or(RangeError::Overflow)?)
            };
            let hi = if h1 <= 0 { 0 } else { h1.min(bound) };
            Ok((lo, hi))
        }
        _ => Err(RangeError::Overflow),
    }
}

impl InferCtx<'_> {
    pub(crate) fn infer_binary(
        &mut self,
        expr: ExprId,
        op: BinaryOp,
        lhs: ExprId,
        rhs: ExprId,
    ) -> Ty {
        match op {
            BinaryOp::And | BinaryOp::Or => {
                self.infer_expr(lhs, Expect::Coerce(&Ty::Bool));
                self.infer_expr(rhs, Expect::Coerce(&Ty::Bool));
                Ty::Bool
            }
            BinaryOp::Eq | BinaryOp::Ne => {
                let l = self.infer_expr(lhs, Expect::None);
                let r = self.infer_expr(rhs, Expect::Hint(&l));
                self.compare(expr, op, &l, &r, true);
                Ty::Bool
            }
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                let l = self.infer_expr(lhs, Expect::None);
                let r = self.infer_expr(rhs, Expect::Hint(&l));
                self.compare(expr, op, &l, &r, false);
                Ty::Bool
            }
            _ => {
                let l = self.infer_expr(lhs, Expect::None);
                let r = self.infer_expr(rhs, Expect::None);
                let range = self.expr_range(expr);
                self.arith(range, op, &l, &r)
            }
        }
    }

    /// Literal operands of an `Int` operation become `Int<n, n>`.
    fn fix_literal_to_int(&mut self, ty: &Ty) {
        if let Some((_, literal)) = self.table.literal(ty) {
            self.table.unify(ty, &Ty::bounded(literal.lo, literal.hi));
        }
    }

    fn compare(&mut self, expr: ExprId, op: BinaryOp, l: &Ty, r: &Ty, eq: bool) {
        let range = self.expr_range(expr);
        let ls = self.structurally_resolve(l);
        let rs = self.structurally_resolve(r);
        let l_lit = self.table.literal(l).is_some();
        let r_lit = self.table.literal(r).is_some();
        let l_bounded = matches!(ls, Ty::Bounded { .. });
        let r_bounded = matches!(rs, Ty::Bounded { .. });
        if matches!(ls, Ty::Error) || matches!(rs, Ty::Error) {
            return;
        }
        if l_lit && r_lit {
            self.link(vec![l.clone(), r.clone()]);
        } else if l_bounded && r_lit {
            self.fix_literal_to_int(r);
        } else if l_lit && r_bounded {
            self.fix_literal_to_int(l);
        } else if l_bounded && r_bounded {
        } else if (l_bounded && matches!(rs, Ty::Int(_))) || (r_bounded && matches!(ls, Ty::Int(_)))
        {
            self.int_mix(range, l, r);
            return;
        } else if !self.try_unify(l, r) {
            self.mismatch(range, l, r);
            return;
        }
        if eq {
            if let Some(eq_trait) = self.sigs.std.eq {
                self.obligations.push(Obligation {
                    ty: l.clone(),
                    trait_item: eq_trait,
                    range,
                    kind: ObligationKind::Eq,
                });
            }
            return;
        }
        let resolved = self.structurally_resolve(l);
        let std = &self.sigs.std;
        let orderable = match &resolved {
            Ty::Int(_) | Ty::Bounded { .. } | Ty::Error | Ty::Infer(_) => true,
            Ty::Adt { item, .. } => Some(*item) == std.decimal || Some(*item) == std.rat,
            _ => false,
        };
        if !orderable {
            let message = format!(
                "`{}` cannot be applied to `{}`",
                op_text(op),
                self.show(&resolved)
            );
            self.diag(range, DiagnosticCode::TypeBadOperands, message);
        }
    }

    fn int_mix(&mut self, range: TextRange, l: &Ty, r: &Ty) {
        let message = format!(
            "cannot mix `{}` and `{}`; convert one side with `.into()` or `.saturate()`",
            self.show(l),
            self.show(r)
        );
        self.diag(range, DiagnosticCode::TypeIntMix, message);
    }

    fn bad_operands(&mut self, range: TextRange, op: BinaryOp, l: &Ty, r: &Ty) {
        let message = format!(
            "cannot apply `{}` to `{}` and `{}`",
            op_text(op),
            self.show(l),
            self.show(r)
        );
        self.diag(range, DiagnosticCode::TypeBadOperands, message);
    }

    /// The result type of an arithmetic or bitwise operator.
    pub(crate) fn arith(&mut self, range: TextRange, op: BinaryOp, l: &Ty, r: &Ty) -> Ty {
        let ls = self.structurally_resolve(l);
        let rs = self.structurally_resolve(r);
        if matches!(ls, Ty::Error) || matches!(rs, Ty::Error) {
            return Ty::Error;
        }
        let l_lit = self.table.literal(l);
        let r_lit = self.table.literal(r);
        let std = &self.sigs.std;
        let exact = |ty: &Ty| matches!(ty, Ty::Adt { item, .. } if Some(*item) == std.decimal || Some(*item) == std.rat);
        match (&ls, &rs) {
            (Ty::Bool, Ty::Bool)
                if matches!(op, BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor) =>
            {
                Ty::Bool
            }
            (Ty::Int(a), Ty::Int(b)) => {
                if !is_shift(op) && a != b {
                    self.mismatch(range, &ls, &rs);
                }
                Ty::Int(*a)
            }
            (Ty::Int(a), _) if r_lit.is_some() => {
                self.table.unify(r, &Ty::Int(*a));
                Ty::Int(*a)
            }
            (_, Ty::Int(b)) if l_lit.is_some() => {
                self.table.unify(l, &Ty::Int(*b));
                Ty::Int(*b)
            }
            _ if l_lit.is_some() && r_lit.is_some() => {
                let (Some((_, a)), Some((_, b))) = (l_lit, r_lit) else {
                    return Ty::Error;
                };
                if is_bitwise(op) {
                    self.table.unify(l, r);
                    return l.clone();
                }
                match range_op(op, (a.lo, a.hi), (b.lo, b.hi)) {
                    Ok((lo, hi)) => {
                        let result = self.table.new_literal(lo, hi, a.origin);
                        self.link(vec![l.clone(), r.clone(), result.clone()]);
                        result
                    }
                    Err(error) => {
                        self.range_error(range, error, r);
                        Ty::Error
                    }
                }
            }
            (Ty::Bounded { .. }, _) | (_, Ty::Bounded { .. })
                if (matches!(ls, Ty::Bounded { .. }) || l_lit.is_some())
                    && (matches!(rs, Ty::Bounded { .. }) || r_lit.is_some()) =>
            {
                self.fix_literal_to_int(l);
                self.fix_literal_to_int(r);
                if is_bitwise(op) {
                    let message = format!(
                        "`{}` needs Rust integers; `{}` is a bounded integer",
                        op_text(op),
                        self.show(l)
                    );
                    self.diag(range, DiagnosticCode::TypeBadOperands, message);
                    return Ty::Error;
                }
                let (Some(a), Some(b)) = (self.bounded_range(l), self.bounded_range(r)) else {
                    let message = format!(
                        "cannot compute the range of `{}` on `{}` and `{}`: their bounds are generic",
                        op_text(op),
                        self.show(l),
                        self.show(r)
                    );
                    self.diag(range, DiagnosticCode::TypeBadOperands, message);
                    return Ty::Error;
                };
                match range_op(op, a, b) {
                    Ok((lo, hi)) => Ty::bounded(lo, hi),
                    Err(error) => {
                        self.range_error(range, error, r);
                        Ty::Error
                    }
                }
            }
            (Ty::Bounded { .. }, Ty::Int(_)) | (Ty::Int(_), Ty::Bounded { .. }) => {
                self.int_mix(range, l, r);
                Ty::Error
            }
            (a, b) if exact(a) && a == b && !is_bitwise(op) => ls.clone(),
            (Ty::Infer(_), _) | (_, Ty::Infer(_)) if l_lit.is_none() || r_lit.is_none() => {
                if !self.try_unify(l, r) {
                    self.bad_operands(range, op, l, r);
                    return Ty::Error;
                }
                l.clone()
            }
            _ => {
                self.bad_operands(range, op, l, r);
                Ty::Error
            }
        }
    }

    fn range_error(&mut self, range: TextRange, error: RangeError, divisor: &Ty) {
        match error {
            RangeError::Zero => {
                let message = format!(
                    "the divisor's range `{}` contains 0; narrow its type first",
                    self.show(divisor)
                );
                self.diag(range, DiagnosticCode::TypeDivisorMayBeZero, message);
            }
            RangeError::Overflow => self.diag(
                range,
                DiagnosticCode::TypeBadOperands,
                "the range of this expression overflows",
            ),
        }
    }

    /// `-x`.
    pub(crate) fn negate(&mut self, expr: ExprId, ty: &Ty) -> Ty {
        let range = self.expr_range(expr);
        let resolved = self.structurally_resolve(ty);
        if let Some((_, literal)) = self.table.literal(ty) {
            let (Some(lo), Some(hi)) = (literal.hi.checked_neg(), literal.lo.checked_neg()) else {
                return Ty::Error;
            };
            let result = self.table.new_literal(lo, hi, expr);
            self.link(vec![ty.clone(), result.clone()]);
            return result;
        }
        let std = &self.sigs.std;
        match &resolved {
            Ty::Error | Ty::Infer(_) => resolved.clone(),
            Ty::Bounded { .. } => match self.bounded_range(&resolved) {
                Some((lo, hi)) => match (hi.checked_neg(), lo.checked_neg()) {
                    (Some(lo), Some(hi)) => Ty::bounded(lo, hi),
                    _ => Ty::Error,
                },
                None => {
                    self.diag(
                        range,
                        DiagnosticCode::TypeBadOperands,
                        "cannot negate an `Int` with generic bounds",
                    );
                    Ty::Error
                }
            },
            Ty::Int(int) if IntTy::min(*int) < 0 => resolved.clone(),
            Ty::Adt { item, .. } if Some(*item) == std.decimal || Some(*item) == std.rat => {
                resolved.clone()
            }
            other => {
                let message = format!("cannot negate `{}`", self.show(other));
                self.diag(range, DiagnosticCode::TypeBadOperands, message);
                Ty::Error
            }
        }
    }

    /// `target = value` and `target op= value`.
    pub(crate) fn infer_assign(
        &mut self,
        expr: ExprId,
        op: Option<BinaryOp>,
        target: ExprId,
        value: ExprId,
    ) {
        let target_ty = self.infer_expr(target, Expect::None);
        let Some(op) = op else {
            self.infer_expr(value, Expect::Coerce(&target_ty));
            return;
        };
        let value_ty = self.infer_expr(value, Expect::None);
        let range = self.expr_range(expr);
        let resolved = self.structurally_resolve(&target_ty);
        if let Ty::Bounded { .. } = resolved {
            // Compound assignment on an `Int` saturates, so any `Int` right side is fine.
            self.fix_literal_to_int(&value_ty);
            let value_resolved = self.structurally_resolve(&value_ty);
            match value_resolved {
                Ty::Bounded { .. } => {
                    if is_bitwise(op) {
                        let message = format!(
                            "`{}=` needs Rust integers; `{}` is a bounded integer",
                            op_text(op),
                            self.show(&target_ty)
                        );
                        self.diag(range, DiagnosticCode::TypeBadOperands, message);
                    } else if matches!(op, BinaryOp::Div | BinaryOp::Rem)
                        && let Some((lo, hi)) = self.bounded_range(&value_ty)
                        && lo <= 0
                        && 0 <= hi
                    {
                        self.range_error(range, RangeError::Zero, &value_ty);
                    }
                }
                Ty::Int(_) => self.int_mix(range, &target_ty, &value_ty),
                Ty::Error => {}
                _ => {
                    let value_range = self.expr_range(value);
                    self.mismatch(value_range, &target_ty, &value_ty);
                }
            }
            return;
        }
        let result = self.arith(range, op, &target_ty, &value_ty);
        if !matches!(self.table.shallow(&result), Ty::Error) && !self.try_unify(&result, &target_ty)
        {
            self.mismatch(range, &target_ty, &result);
        }
    }
}
