//! Inference of expressions and statements.

use super::ClosureInfo;
use super::ctx::{Expect, InferCtx, LoopState, ptr_to_closure_kind};
use crate::diagnostic::DiagnosticCode;
use crate::hir::{ArrayExpr, Condition, Expr, ExprId, Literal, PatId, Stmt, UnaryOp};
use crate::resolve::{LocalId, Resolution};
use crate::ty::{Const, GenericArg, IntTy, Ty};

impl InferCtx<'_> {
    /// Infers `expr`; with `Expect::Coerce` the result is the target type.
    pub(crate) fn infer_expr(&mut self, expr: ExprId, expect: Expect<'_>) -> Ty {
        let (ty, coerced) = self.infer_expr_inner(expr, expect);
        self.record_expr(expr, &ty);
        match expect {
            Expect::Coerce(target) if !coerced => {
                self.coerce(expr, &ty, target);
                target.clone()
            }
            _ => ty,
        }
    }

    /// The type of `expr` and whether it already coerced to the expected type (blocks, `if`,
    /// `match`, tuples propagate the coercion into their parts).
    fn infer_expr_inner(&mut self, expr: ExprId, expect: Expect<'_>) -> (Ty, bool) {
        let store = self.store;
        let ty = match store.expr(expr) {
            Expr::Missing => Ty::Error,
            Expr::Literal(literal) => self.infer_literal(expr, literal),
            Expr::Path(path) => self.infer_path(expr, path),
            Expr::Block {
                stmts, tail, scope, ..
            } => return self.infer_block(stmts, *tail, *scope, expect),
            Expr::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.infer_condition(*cond);
                let Some(else_branch) = else_branch else {
                    self.infer_expr(*then_branch, Expect::Coerce(&Ty::unit()));
                    return (Ty::unit(), false);
                };
                if let Expect::Coerce(target) = expect {
                    self.infer_expr(*then_branch, Expect::Coerce(target));
                    self.infer_expr(*else_branch, Expect::Coerce(target));
                    return (target.clone(), true);
                }
                let then_ty = self.infer_expr(*then_branch, expect);
                let else_ty = self.infer_expr(*else_branch, expect);
                self.join_arms(&[(*then_branch, then_ty), (*else_branch, else_ty)])
            }
            Expr::Match { scrutinee, arms } => {
                let scrutinee_ty = self.infer_expr(*scrutinee, Expect::None);
                let mut arm_tys = Vec::new();
                for arm in arms {
                    self.check_pat(arm.pat, &scrutinee_ty);
                    if let Some(guard) = arm.guard {
                        self.infer_expr(guard, Expect::Coerce(&Ty::Bool));
                    }
                    let ty = self.infer_expr(arm.body, expect);
                    arm_tys.push((arm.body, ty));
                }
                if let Expect::Coerce(target) = expect {
                    return (
                        if arms.is_empty() {
                            Ty::Never
                        } else {
                            target.clone()
                        },
                        !arms.is_empty(),
                    );
                }
                self.join_arms(&arm_tys)
            }
            Expr::Loop { body, .. } => {
                let ty = self.table.new_var();
                self.loops.insert(
                    expr,
                    LoopState {
                        ty: Some(ty.clone()),
                        has_break: false,
                    },
                );
                self.infer_expr(*body, Expect::Coerce(&Ty::unit()));
                match self.loops.remove(&expr) {
                    Some(state) if state.has_break => ty,
                    _ => Ty::Never,
                }
            }
            Expr::While { cond, body, .. } => {
                self.infer_condition(*cond);
                self.loops.insert(
                    expr,
                    LoopState {
                        ty: None,
                        has_break: false,
                    },
                );
                self.infer_expr(*body, Expect::Coerce(&Ty::unit()));
                self.loops.remove(&expr);
                Ty::unit()
            }
            Expr::For {
                pat,
                iterable,
                body,
                ..
            } => {
                self.infer_for(*pat, *iterable);
                self.loops.insert(
                    expr,
                    LoopState {
                        ty: None,
                        has_break: false,
                    },
                );
                self.infer_expr(*body, Expect::Coerce(&Ty::unit()));
                self.loops.remove(&expr);
                Ty::unit()
            }
            Expr::Break { value, .. } => {
                self.infer_break(expr, *value);
                Ty::Never
            }
            Expr::Continue { .. } => Ty::Never,
            Expr::Return { value } => {
                let ret = self.returns.last().cloned().unwrap_or(Ty::Error);
                match value {
                    Some(value) => {
                        self.infer_expr(*value, Expect::Coerce(&ret));
                    }
                    None => {
                        if !self.try_unify(&ret, &Ty::unit()) {
                            let range = self.expr_range(expr);
                            self.mismatch(range, &ret, &Ty::unit());
                        }
                    }
                }
                Ty::Never
            }
            Expr::Call { callee, args } => self.infer_call(expr, *callee, args, expect),
            Expr::MethodCall {
                receiver,
                name,
                args_generic,
                args,
            } => self.infer_method_call(expr, *receiver, name, args_generic.as_ref(), args, expect),
            Expr::Field { base, name } => self.infer_field(expr, *base, name),
            Expr::Index { base, index } => self.infer_index(expr, *base, *index),
            Expr::Try(inner) => self.infer_try(expr, *inner),
            Expr::Cast { expr: inner, ty } => {
                self.infer_expr(*inner, Expect::None);
                let target = self.lower_type(*ty);
                let range = self.expr_range(expr);
                self.diag(
                    range,
                    DiagnosticCode::TypeAsCast,
                    "there are no `as` casts; use `.into()` or `.saturate()`",
                );
                target
            }
            Expr::Unary { op, expr: inner } => self.infer_unary(expr, *op, *inner),
            Expr::Ref {
                mutable,
                expr: inner,
            } => {
                let hint = match expect.ty().map(|ty| self.table.shallow(ty)) {
                    Some(Ty::Ref { inner, .. }) => Some(*inner),
                    _ => None,
                };
                let inner_ty = match &hint {
                    Some(hint) => self.infer_expr(*inner, Expect::Hint(hint)),
                    None => self.infer_expr(*inner, Expect::None),
                };
                Ty::Ref {
                    mutable: *mutable,
                    inner: Box::new(inner_ty),
                }
            }
            Expr::Binary { op, lhs, rhs } => self.infer_binary(expr, *op, *lhs, *rhs),
            Expr::Assign { op, target, value } => {
                self.infer_assign(expr, *op, *target, *value);
                Ty::unit()
            }
            Expr::Range {
                start,
                end,
                inclusive: _,
            } => self.infer_range(expr, *start, *end, expect),
            Expr::Tuple(exprs) => {
                let expected = match expect.ty().map(|ty| self.table.shallow(ty)) {
                    Some(Ty::Tuple(tys)) if tys.len() == exprs.len() => Some(tys),
                    _ => None,
                };
                match (expected, expect) {
                    (Some(tys), Expect::Coerce(target)) => {
                        for (expr, ty) in exprs.iter().zip(&tys) {
                            self.infer_expr(*expr, Expect::Coerce(ty));
                        }
                        return (target.clone(), true);
                    }
                    (Some(tys), _) => Ty::Tuple(
                        exprs
                            .iter()
                            .zip(&tys)
                            .map(|(expr, ty)| self.infer_expr(*expr, Expect::Hint(ty)))
                            .collect(),
                    ),
                    (None, _) => Ty::Tuple(
                        exprs
                            .iter()
                            .map(|expr| self.infer_expr(*expr, Expect::None))
                            .collect(),
                    ),
                }
            }
            Expr::Array(ArrayExpr::List(exprs)) => self.infer_array_list(exprs, expect),
            Expr::Array(ArrayExpr::Repeat { value, count }) => {
                let elem = match expect.ty().map(|ty| self.table.shallow(ty)) {
                    Some(Ty::Array { elem, .. }) => {
                        self.infer_expr(*value, Expect::Coerce(&elem));
                        *elem
                    }
                    _ => self.infer_expr(*value, Expect::None),
                };
                self.infer_expr(*count, Expect::Coerce(&Ty::Int(IntTy::Usize)));
                let scope = self.scope();
                let mut sink = crate::signatures::lower::Sink::loud();
                let len = self.lowerer.fold(scope, *count, &mut sink);
                self.diags.extend(sink.diags);
                Ty::Array {
                    elem: Box::new(elem),
                    len,
                }
            }
            Expr::Record { path, fields } => self.infer_record(expr, path, fields, expect),
            Expr::Closure { params, ret, body } => {
                self.infer_closure(expr, params, *ret, *body, expect)
            }
        };
        (ty, false)
    }

    fn infer_literal(&mut self, expr: ExprId, literal: &Literal) -> Ty {
        match literal {
            Literal::Int { value, suffix } => {
                self.int_literal(expr, *value, suffix.as_deref(), false)
            }
            Literal::Decimal(_) => self.std_adt(self.sigs.std.decimal),
            Literal::String(_) => self.std_adt(self.sigs.std.text),
            Literal::Bool(_) => Ty::Bool,
        }
    }

    pub(crate) fn std_adt(&self, item: Option<crate::item_tree::ItemId>) -> Ty {
        match item {
            Some(item) => Ty::Adt {
                item,
                args: Vec::new(),
            },
            None => Ty::Error,
        }
    }

    /// An integer literal (negated for `-5`): a literal variable, or the suffix's type.
    pub(crate) fn int_literal(
        &mut self,
        expr: ExprId,
        value: u128,
        suffix: Option<&str>,
        negated: bool,
    ) -> Ty {
        let range = self.expr_range(expr);
        let value = i128::try_from(value).ok().and_then(|value| {
            if negated {
                value.checked_neg()
            } else {
                Some(value)
            }
        });
        let Some(value) = value else {
            self.diag(
                range,
                DiagnosticCode::TypeLiteralOutOfRange,
                "integer literal is too large",
            );
            return Ty::Error;
        };
        match suffix {
            None => self.table.new_literal(value, value, expr),
            Some(suffix) => match IntTy::from_suffix(suffix) {
                Some(int) => {
                    if value < int.min() || value > int.max() {
                        self.diag(
                            range,
                            DiagnosticCode::TypeLiteralOutOfRange,
                            format!(
                                "literal out of range for `{}`: `{value}` does not fit",
                                int.name()
                            ),
                        );
                    }
                    Ty::Int(int)
                }
                None => {
                    self.diag(
                        range,
                        DiagnosticCode::TypeBadLiteral,
                        format!("invalid suffix `{suffix}` for an integer literal"),
                    );
                    Ty::Error
                }
            },
        }
    }

    fn infer_block(
        &mut self,
        stmts: &[Stmt],
        tail: Option<ExprId>,
        scope: Option<u32>,
        expect: Expect<'_>,
    ) -> (Ty, bool) {
        let module = scope.and_then(|block| {
            self.maps
                .block_modules
                .get(&(self.item.file, block))
                .copied()
        });
        if let Some(module) = module {
            self.modules.push(module);
        }
        let mut diverges = false;
        for stmt in stmts {
            match *stmt {
                Stmt::Let { pat, ty, init } => {
                    let declared = ty.map(|ty| self.lower_type(ty));
                    let ty = match (init, declared) {
                        (Some(init), Some(declared)) => {
                            let actual = self.infer_expr(init, Expect::Coerce(&declared));
                            let _ = actual;
                            diverges |= self.is_never_expr(init);
                            declared
                        }
                        (Some(init), None) => {
                            let ty = self.infer_expr(init, Expect::None);
                            diverges |= self.is_never_expr(init);
                            ty
                        }
                        (None, Some(declared)) => declared,
                        (None, None) => self.table.new_var(),
                    };
                    self.check_pat(pat, &ty);
                }
                Stmt::Expr { expr, has_semi } => {
                    let ty = self.infer_expr(expr, Expect::None);
                    let never = matches!(self.structurally_resolve(&ty), Ty::Never);
                    diverges |= never;
                    if !has_semi && !never {
                        self.coerce(expr, &ty, &Ty::unit());
                    }
                }
                Stmt::Item(_) => {}
            }
        }
        let out = match tail {
            Some(tail) => {
                let ty = self.infer_expr(tail, expect);
                (ty, matches!(expect, Expect::Coerce(_)))
            }
            None if diverges => (Ty::Never, false),
            None => (Ty::unit(), false),
        };
        if module.is_some() {
            self.modules.pop();
        }
        out
    }

    fn is_never_expr(&self, expr: ExprId) -> bool {
        self.result
            .expr_types
            .get(expr)
            .is_some_and(|ty| matches!(self.table.shallow(ty), Ty::Never))
    }

    pub(crate) fn infer_condition(&mut self, cond: Condition) {
        match cond {
            Condition::Expr(expr) => {
                self.infer_expr(expr, Expect::Coerce(&Ty::Bool));
            }
            Condition::Let { pat, expr } => {
                let ty = self.infer_expr(expr, Expect::None);
                self.check_pat(pat, &ty);
            }
        }
    }

    fn infer_break(&mut self, expr: ExprId, value: Option<ExprId>) {
        let target = self.res.loop_targets.get(&expr).copied();
        let loop_ty = target.and_then(|target| {
            let state = self.loops.get_mut(&target)?;
            state.has_break = true;
            Some(state.ty.clone())
        });
        match (loop_ty, value) {
            (Some(Some(ty)), Some(value)) => {
                self.infer_expr(value, Expect::Coerce(&ty));
            }
            (Some(Some(ty)), None) => {
                if !self.try_unify(&ty, &Ty::unit()) {
                    let range = self.expr_range(expr);
                    self.mismatch(range, &ty, &Ty::unit());
                }
            }
            (Some(None), Some(value)) => {
                let range = self.expr_range(value);
                self.diag(
                    range,
                    DiagnosticCode::TypeMismatch,
                    "`break` with a value is only allowed in `loop`",
                );
                self.infer_expr(value, Expect::None);
            }
            (None, Some(value)) => {
                self.infer_expr(value, Expect::None);
            }
            (_, None) => {}
        }
    }

    fn infer_for(&mut self, pat: PatId, iterable: ExprId) {
        let ty = self.infer_expr(iterable, Expect::None);
        let resolved = self.structurally_resolve(&ty);
        let literal_range = matches!(self.store.expr(iterable), Expr::Range { .. });
        let inclusive = matches!(
            self.store.expr(iterable),
            Expr::Range {
                inclusive: true,
                ..
            }
        );
        let std = &self.sigs.std;
        let item = match &resolved {
            Ty::Error => Ty::Error,
            Ty::Array { elem, .. } => (**elem).clone(),
            Ty::Adt { item, args } if Some(*item) == std.vec => match args.first() {
                Some(GenericArg::Type(elem)) => elem.clone(),
                _ => Ty::Error,
            },
            Ty::Adt { item, args } if Some(*item) == std.range => {
                let elem = match args.first() {
                    Some(GenericArg::Type(elem)) => elem.clone(),
                    _ => Ty::Error,
                };
                self.range_item(iterable, &ty, &elem, literal_range, inclusive)
            }
            Ty::Infer(_) => {
                let range = self.expr_range(iterable);
                self.diag(
                    range,
                    DiagnosticCode::TypeAnnotationsNeeded,
                    "type annotations needed: the type of the iterated value is unknown",
                );
                Ty::Error
            }
            other => {
                let range = self.expr_range(iterable);
                let message = format!(
                    "`{}` is not iterable: `for` loops over arrays, `Vec` and integer ranges",
                    self.show(other)
                );
                self.diag(range, DiagnosticCode::TypeNotIterable, message);
                Ty::Error
            }
        };
        self.check_pat(pat, &item);
    }

    /// The item type of a `for` loop over a `Range<elem>`.
    fn range_item(
        &mut self,
        iterable: ExprId,
        range_ty: &Ty,
        elem: &Ty,
        literal_range: bool,
        inclusive: bool,
    ) -> Ty {
        let shallow = self.structurally_resolve(elem);
        match &shallow {
            Ty::Error | Ty::Int(_) => shallow.clone(),
            Ty::Bounded { lo, hi } => {
                let (Some(lo), Some(hi)) = (self.const_value(lo), self.const_value(hi)) else {
                    return elem.clone();
                };
                if !literal_range || inclusive {
                    return elem.clone();
                }
                Ty::bounded(lo, hi.saturating_sub(1).max(lo))
            }
            Ty::Infer(_) => match self.table.literal(elem) {
                Some((_, literal)) if literal_range => {
                    let hi = if inclusive {
                        literal.hi
                    } else {
                        literal.hi.saturating_sub(1).max(literal.lo)
                    };
                    let item = self.table.new_literal(literal.lo, hi, literal.origin);
                    self.link(vec![elem.clone(), item.clone()]);
                    item
                }
                Some(_) => elem.clone(),
                None => elem.clone(),
            },
            _ => {
                let range = self.expr_range(iterable);
                let message = format!(
                    "`{}` is not iterable: only ranges of integers are",
                    self.show(range_ty)
                );
                self.diag(range, DiagnosticCode::TypeNotIterable, message);
                Ty::Error
            }
        }
    }

    fn infer_try(&mut self, expr: ExprId, inner: ExprId) -> Ty {
        let ty = self.infer_expr(inner, Expect::None);
        let resolved = self.structurally_resolve(&ty);
        let std = &self.sigs.std;
        let (option, result) = (std.option, std.result);
        let ret = self.returns.last().cloned().unwrap_or(Ty::Error);
        let ret_resolved = self.structurally_resolve(&ret);
        let range = self.expr_range(expr);
        match &resolved {
            Ty::Error => Ty::Error,
            Ty::Adt { item, args } if Some(*item) == option => {
                let value = match args.first() {
                    Some(GenericArg::Type(value)) => value.clone(),
                    _ => Ty::Error,
                };
                match &ret_resolved {
                    Ty::Adt { item, .. } if Some(*item) == option => {}
                    Ty::Error => {}
                    Ty::Infer(_) => {
                        let fresh = self.table.new_var();
                        self.table.unify(
                            &ret,
                            &Ty::Adt {
                                item: *item,
                                args: vec![GenericArg::Type(fresh)],
                            },
                        );
                    }
                    _ => self.diag(
                        range,
                        DiagnosticCode::TypeBadTry,
                        "the `?` operator on an `Option` needs a function that returns `Option`",
                    ),
                }
                value
            }
            Ty::Adt { item, args } if Some(*item) == result => {
                let (value, error) = match args.as_slice() {
                    [GenericArg::Type(value), GenericArg::Type(error)] => {
                        (value.clone(), error.clone())
                    }
                    _ => (Ty::Error, Ty::Error),
                };
                match &ret_resolved {
                    Ty::Adt { item, args } if Some(*item) == result => {
                        if let Some(GenericArg::Type(ret_error)) = args.get(1)
                            && !self.try_unify(&error, ret_error)
                        {
                            self.mismatch(range, ret_error, &error);
                        }
                    }
                    Ty::Error => {}
                    Ty::Infer(_) => {
                        let fresh = self.table.new_var();
                        self.table.unify(
                            &ret,
                            &Ty::Adt {
                                item: *item,
                                args: vec![GenericArg::Type(fresh), GenericArg::Type(error)],
                            },
                        );
                    }
                    _ => self.diag(
                        range,
                        DiagnosticCode::TypeBadTry,
                        "the `?` operator on a `Result` needs a function that returns `Result`",
                    ),
                }
                value
            }
            other => {
                let message = format!(
                    "the `?` operator applies to `Option` and `Result`, not `{}`",
                    self.show(other)
                );
                self.diag(range, DiagnosticCode::TypeBadTry, message);
                Ty::Error
            }
        }
    }

    fn infer_array_list(&mut self, exprs: &[ExprId], expect: Expect<'_>) -> Ty {
        let len = Const::Value(i128::try_from(exprs.len()).unwrap_or(i128::MAX));
        let expected_elem = match expect.ty().map(|ty| self.table.shallow(ty)) {
            Some(Ty::Array { elem, .. }) => Some(*elem),
            _ => None,
        };
        let elem = match expected_elem {
            Some(elem) => {
                for expr in exprs {
                    self.infer_expr(*expr, Expect::Coerce(&elem));
                }
                elem
            }
            None => {
                let tys: Vec<(ExprId, Ty)> = exprs
                    .iter()
                    .map(|expr| (*expr, self.infer_expr(*expr, Expect::None)))
                    .collect();
                if tys.is_empty() {
                    self.table.new_var()
                } else {
                    self.join_arms(&tys)
                }
            }
        };
        Ty::Array {
            elem: Box::new(elem),
            len,
        }
    }

    fn infer_range(
        &mut self,
        expr: ExprId,
        start: Option<ExprId>,
        end: Option<ExprId>,
        expect: Expect<'_>,
    ) -> Ty {
        let (Some(start), Some(end)) = (start, end) else {
            for bound in [start, end].into_iter().flatten() {
                self.infer_expr(bound, Expect::None);
            }
            let range = self.expr_range(expr);
            self.diag(
                range,
                DiagnosticCode::TypeOpenRange,
                "a range expression needs both bounds; `a..`, `..b` and `..` exist only in patterns",
            );
            return Ty::Error;
        };
        let Some(range_item) = self.sigs.std.range else {
            self.infer_expr(start, Expect::None);
            self.infer_expr(end, Expect::None);
            return Ty::Error;
        };
        let expected_elem = match expect.ty().map(|ty| self.table.shallow(ty)) {
            Some(Ty::Adt { item, args }) if item == range_item => match args.first() {
                Some(GenericArg::Type(elem))
                    if !matches!(self.table.shallow(elem), Ty::Infer(_)) =>
                {
                    Some(elem.clone())
                }
                _ => None,
            },
            _ => None,
        };
        let elem = match expected_elem {
            Some(elem) => {
                self.infer_expr(start, Expect::Coerce(&elem));
                self.infer_expr(end, Expect::Coerce(&elem));
                elem
            }
            None => {
                let start_ty = self.infer_expr(start, Expect::None);
                let end_ty = self.infer_expr(end, Expect::None);
                self.join_arms(&[(start, start_ty), (end, end_ty)])
            }
        };
        Ty::Adt {
            item: range_item,
            args: vec![GenericArg::Type(elem)],
        }
    }

    fn infer_closure(
        &mut self,
        expr: ExprId,
        params: &[(PatId, Option<crate::hir::TypeRefId>)],
        ret: Option<crate::hir::TypeRefId>,
        body: ExprId,
        expect: Expect<'_>,
    ) -> Ty {
        let expected = expect.ty().map(|ty| self.structurally_resolve(ty));
        let (expected_kind, expected_params, expected_ret) = match expected {
            Some(Ty::Closure { kind, params, ret }) => (Some(kind), Some(params), Some(*ret)),
            Some(Ty::FnPtr { kind, params, ret }) => {
                (ptr_to_closure_kind(kind), Some(params), Some(*ret))
            }
            _ => (None, None, None),
        };
        let kind = expected_kind
            .or_else(|| self.closure_kinds.last().copied())
            .unwrap_or(self.fn_kind);
        let expected_params = expected_params.filter(|expected| {
            if expected.len() == params.len() {
                return true;
            }
            let range = self.expr_range(expr);
            self.diag(
                range,
                DiagnosticCode::TypeWrongArity,
                format!(
                    "expected a closure that takes {}, found one that takes {}",
                    crate::signatures::lower::plural(expected.len(), "argument"),
                    crate::signatures::lower::plural(params.len(), "argument"),
                ),
            );
            false
        });
        let mut param_tys = Vec::new();
        for (index, &(pat, annotation)) in params.iter().enumerate() {
            let declared = annotation.map(|ty| self.lower_type(ty));
            let expected = expected_params
                .as_ref()
                .and_then(|expected| expected.get(index))
                .cloned();
            let ty = match (declared, expected) {
                (Some(declared), Some(expected)) => {
                    if !self.try_unify(&declared, &expected) {
                        let range = self.pat_range(pat);
                        self.mismatch(range, &expected, &declared);
                    }
                    declared
                }
                (Some(declared), None) => declared,
                (None, Some(expected)) => expected,
                (None, None) => self.table.new_var(),
            };
            self.check_pat(pat, &ty);
            param_tys.push(ty);
        }
        let ret_ty = match ret {
            Some(ret) => {
                let declared = self.lower_type(ret);
                if let Some(expected) = &expected_ret
                    && !self.try_unify(&declared, expected)
                {
                    let range = self.expr_range(expr);
                    self.mismatch(range, expected, &declared);
                }
                declared
            }
            None => expected_ret.unwrap_or_else(|| self.table.new_var()),
        };
        let captures = self.captures(expr, params, body);
        self.result
            .closures
            .insert(expr, ClosureInfo { kind, captures });
        self.returns.push(ret_ty.clone());
        self.closure_kinds.push(kind);
        self.infer_expr(body, Expect::Coerce(&ret_ty));
        self.closure_kinds.pop();
        self.returns.pop();
        Ty::Closure {
            kind,
            params: param_tys,
            ret: Box::new(ret_ty),
        }
    }

    /// Locals declared outside the closure that its body uses, in order of first use.
    fn captures(
        &self,
        _closure: ExprId,
        params: &[(PatId, Option<crate::hir::TypeRefId>)],
        body: ExprId,
    ) -> Vec<LocalId> {
        let mut inner: Vec<LocalId> = Vec::new();
        let mut used: Vec<LocalId> = Vec::new();
        let mut pats: Vec<PatId> = params.iter().map(|(pat, _)| *pat).collect();
        let mut exprs = vec![body];
        while let Some(expr) = exprs.pop() {
            if let Expr::Path(_) = self.store.expr(expr)
                && let Some(Resolution::Local(local)) = self.res.store.exprs.get(&expr)
            {
                used.push(*local);
            }
            super::walk::expr_children(
                self.store.expr(expr),
                &mut |child| exprs.push(child),
                &mut |pat| {
                    pats.push(pat);
                },
            );
        }
        while let Some(pat) = pats.pop() {
            if let Some(local) = self.res.bindings.get(&pat) {
                inner.push(*local);
            }
            super::walk::pat_children(self.store.pat(pat), &mut |child| pats.push(child));
        }
        // `used` was collected depth-first from the end; restore source order.
        used.sort_by_key(|local| self.first_use(body, *local));
        let mut captures = Vec::new();
        for local in used {
            if !inner.contains(&local) && !captures.contains(&local) {
                captures.push(local);
            }
        }
        captures
    }

    /// The smallest expression id that uses `local` inside `root`, for ordering captures.
    fn first_use(&self, root: ExprId, local: LocalId) -> u32 {
        let mut best = u32::MAX;
        let mut exprs = vec![root];
        while let Some(expr) = exprs.pop() {
            if let Some(Resolution::Local(used)) = self.res.store.exprs.get(&expr)
                && *used == local
            {
                best = best.min(expr.into_raw().into_u32());
            }
            super::walk::expr_children(
                self.store.expr(expr),
                &mut |child| exprs.push(child),
                &mut |_| {},
            );
        }
        best
    }

    pub(crate) fn infer_unary(&mut self, expr: ExprId, op: UnaryOp, inner: ExprId) -> Ty {
        match op {
            UnaryOp::Neg => {
                if let Expr::Literal(Literal::Int { value, suffix }) = self.store.expr(inner) {
                    let ty = self.int_literal(expr, *value, suffix.as_deref(), true);
                    self.record_expr(inner, &ty);
                    return ty;
                }
                let ty = self.infer_expr(inner, Expect::None);
                self.negate(expr, &ty)
            }
            UnaryOp::Not => {
                let ty = self.infer_expr(inner, Expect::None);
                match self.structurally_resolve(&ty) {
                    Ty::Bool | Ty::Int(_) | Ty::Error | Ty::Infer(_) => ty,
                    other => {
                        let range = self.expr_range(expr);
                        let message = format!("cannot apply `!` to `{}`", self.show(&other));
                        self.diag(range, DiagnosticCode::TypeBadOperands, message);
                        Ty::Error
                    }
                }
            }
            UnaryOp::Deref => {
                let ty = self.infer_expr(inner, Expect::None);
                match self.structurally_resolve(&ty) {
                    Ty::Ref { inner, .. } => *inner,
                    Ty::Error => Ty::Error,
                    other => {
                        let range = self.expr_range(expr);
                        let message = format!(
                            "`{}` cannot be dereferenced: it is not a reference",
                            self.show(&other)
                        );
                        self.diag(range, DiagnosticCode::TypeBadDeref, message);
                        Ty::Error
                    }
                }
            }
        }
    }
}
