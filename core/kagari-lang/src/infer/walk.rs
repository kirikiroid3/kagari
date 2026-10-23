//! The direct children of HIR expressions and patterns.

use crate::hir::{ArrayExpr, Condition, Expr, ExprId, GenericArg, Pat, PatId, Stmt};

fn condition(cond: Condition, on_expr: &mut dyn FnMut(ExprId), on_pat: &mut dyn FnMut(PatId)) {
    match cond {
        Condition::Expr(expr) => on_expr(expr),
        Condition::Let { pat, expr } => {
            on_pat(pat);
            on_expr(expr);
        }
    }
}

/// Calls `on_expr` for each direct sub-expression of `expr` and `on_pat` for each pattern it
/// binds directly (`let`, `match` arms, `if let`, `for`, closure parameters).
pub(crate) fn expr_children(
    expr: &Expr,
    on_expr: &mut dyn FnMut(ExprId),
    on_pat: &mut dyn FnMut(PatId),
) {
    match expr {
        Expr::Missing | Expr::Literal(_) | Expr::Continue { .. } => {}
        Expr::Path(path) => {
            for segment in &path.segments {
                if let Some(args) = &segment.args {
                    for arg in &args.0 {
                        if let GenericArg::Const(expr) = arg {
                            on_expr(*expr);
                        }
                    }
                }
            }
        }
        Expr::Block { stmts, tail, .. } => {
            for stmt in stmts {
                match *stmt {
                    Stmt::Let { pat, init, .. } => {
                        on_pat(pat);
                        if let Some(init) = init {
                            on_expr(init);
                        }
                    }
                    Stmt::Expr { expr, .. } => on_expr(expr),
                    Stmt::Item(_) => {}
                }
            }
            if let Some(tail) = tail {
                on_expr(*tail);
            }
        }
        Expr::If {
            cond,
            then_branch,
            else_branch,
        } => {
            condition(*cond, on_expr, on_pat);
            on_expr(*then_branch);
            if let Some(else_branch) = else_branch {
                on_expr(*else_branch);
            }
        }
        Expr::Match { scrutinee, arms } => {
            on_expr(*scrutinee);
            for arm in arms {
                on_pat(arm.pat);
                if let Some(guard) = arm.guard {
                    on_expr(guard);
                }
                on_expr(arm.body);
            }
        }
        Expr::Loop { body, .. } => on_expr(*body),
        Expr::While { cond, body, .. } => {
            condition(*cond, on_expr, on_pat);
            on_expr(*body);
        }
        Expr::For {
            pat,
            iterable,
            body,
            ..
        } => {
            on_pat(*pat);
            on_expr(*iterable);
            on_expr(*body);
        }
        Expr::Break { value, .. } | Expr::Return { value } => {
            if let Some(value) = value {
                on_expr(*value);
            }
        }
        Expr::Call { callee, args } => {
            on_expr(*callee);
            for arg in args {
                on_expr(*arg);
            }
        }
        Expr::MethodCall { receiver, args, .. } => {
            on_expr(*receiver);
            for arg in args {
                on_expr(*arg);
            }
        }
        Expr::Field { base, .. } => on_expr(*base),
        Expr::Index { base, index } => {
            on_expr(*base);
            on_expr(*index);
        }
        Expr::Try(inner)
        | Expr::Cast { expr: inner, .. }
        | Expr::Unary { expr: inner, .. }
        | Expr::Ref { expr: inner, .. } => on_expr(*inner),
        Expr::Binary { lhs, rhs, .. } => {
            on_expr(*lhs);
            on_expr(*rhs);
        }
        Expr::Assign { target, value, .. } => {
            on_expr(*target);
            on_expr(*value);
        }
        Expr::Range { start, end, .. } => {
            for bound in [start, end].into_iter().flatten() {
                on_expr(*bound);
            }
        }
        Expr::Tuple(exprs) | Expr::Array(ArrayExpr::List(exprs)) => {
            for expr in exprs {
                on_expr(*expr);
            }
        }
        Expr::Array(ArrayExpr::Repeat { value, count }) => {
            on_expr(*value);
            on_expr(*count);
        }
        Expr::Record { fields, .. } => {
            for (_, value) in fields {
                on_expr(*value);
            }
        }
        Expr::Closure { params, body, .. } => {
            for (pat, _) in params {
                on_pat(*pat);
            }
            on_expr(*body);
        }
    }
}

pub(crate) fn pat_children(pat: &Pat, on_pat: &mut dyn FnMut(PatId)) {
    match pat {
        Pat::Tuple(pats) | Pat::TupleStruct { pats, .. } | Pat::Or(pats) => {
            for pat in pats {
                on_pat(*pat);
            }
        }
        Pat::Record { fields, .. } => {
            for (_, pat) in fields {
                on_pat(*pat);
            }
        }
        Pat::Missing
        | Pat::Wild
        | Pat::Bind { .. }
        | Pat::Path(_)
        | Pat::Lit { .. }
        | Pat::Range { .. }
        | Pat::Rest => {}
    }
}
