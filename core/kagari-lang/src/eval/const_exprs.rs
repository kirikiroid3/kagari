//! Which expressions of a body are constant expressions
//! (`docs/syntax/src/language.md#constant-expressions`).
//!
//! An expression is constant when it is:
//! - a literal;
//! - a path to a `const` item, a unit struct or variant, a named function or tuple
//!   constructor, a `const` generic parameter, an immutable `static` (a `Setting`/`Meta`
//!   handle: its value is the handle's identity), or a constant local (below);
//! - a struct, enum, tuple, array or range literal, an operator (unary, binary, cast, index,
//!   shared borrow `&`), a field access, a block, an `if` or a `match` whose operands are all
//!   constant (a block's statements: `let` with a constant initializer binding only
//!   immutable locals, items, and constant expression statements);
//! - a call of a `const fn` (or a tuple constructor) whose arguments are all constant; for a
//!   method call the receiver counts as an argument and must not be borrowed mutably;
//! - `asset::<A>("…")`, which is such a call;
//! - a closure that captures nothing.
//!
//! A **constant local** is an immutable binding of a `let` whose initializer is constant,
//! or of an `if let`/`match` arm pattern whose scrutinee is constant. Parameters, story
//! variables (`static mut`) and the results of `fn` and `pushdown fn` calls are never
//! constant; neither are loops, `break`, `continue`, `return`, `?` and assignments.
//!
//! Expressions of type `Ty::Error` are never constant, so nothing built on an error is
//! evaluated.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::db::Db;
use crate::hir::{ArrayExpr, Body, Condition, Expr, ExprId, ExprStore, Pat, PatId, Stmt, body_of};
use crate::infer::{InferenceResult, ValueRef, inference};
use crate::input::Program;
use crate::item_tree::{FnKind, ItemId, ItemKind, item_tree};
use crate::resolve::{BodyResolution, LocalId, body_resolution};
use crate::ty::FnDefId;

/// The constant expressions of one body.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConstExprs {
    pub exprs: FxHashSet<ExprId>,
    /// Constant locals and where their value comes from.
    pub locals: FxHashMap<LocalId, ConstLocal>,
}

impl ConstExprs {
    pub fn contains(&self, expr: ExprId) -> bool {
        self.exprs.contains(&expr)
    }
}

/// A constant local: bound by `pat` (a `let`, `if let` or `match` arm pattern) matched
/// against the constant expression `source`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConstLocal {
    pub pat: PatId,
    pub source: ExprId,
}

/// The constant expressions of `item`'s body (see the module docs); `Some` where the
/// item has a body with resolution and inference results.
pub fn const_exprs(db: &dyn Db, program: Program, item: ItemId) -> Option<&ConstExprs> {
    item_const_exprs(db, program, item)
}

/// The query behind [`const_exprs`]; a tracked fn named `const_exprs` would clash
/// with this module's name in `eval`.
#[salsa::tracked(returns(as_ref))]
fn item_const_exprs(db: &dyn Db, program: Program, item: ItemId) -> Option<ConstExprs> {
    let body = body_of(db, item)?;
    let res = body_resolution(db, program, item).body.as_ref()?;
    let infer = inference(db, program, item)?;
    Some(analyze(db, body, res, infer))
}

/// The constant expressions of one body.
pub(crate) fn analyze(
    db: &dyn Db,
    body: &Body,
    res: &BodyResolution,
    infer: &InferenceResult,
) -> ConstExprs {
    let mut analyzer = Analyzer {
        db,
        store: &body.store,
        res,
        infer,
        out: ConstExprs::default(),
    };
    analyzer.expr(body.value);
    analyzer.out
}

fn fn_kind(db: &dyn Db, item: ItemId) -> Option<FnKind> {
    match item_tree(db, item.file).item(item.index)?.kind {
        ItemKind::Fn { kind, .. } => Some(kind),
        _ => None,
    }
}

/// Whether a call of `def` with constant arguments is constant.
fn const_callee(db: &dyn Db, def: FnDefId) -> bool {
    match def {
        FnDefId::Item(item) => fn_kind(db, item) == Some(FnKind::Const),
        FnDefId::StructCtor(_) | FnDefId::VariantCtor(..) => true,
    }
}

struct Analyzer<'a> {
    db: &'a dyn Db,
    store: &'a ExprStore,
    res: &'a BodyResolution,
    infer: &'a InferenceResult,
    out: ConstExprs,
}

impl Analyzer<'_> {
    /// Visits `expr` and everything inside it; returns whether `expr` is constant.
    fn expr(&mut self, expr: ExprId) -> bool {
        let constant = self.expr_inner(expr)
            && self
                .infer
                .expr_types
                .get(expr)
                .is_some_and(|ty| !ty.is_error());
        if constant {
            self.out.exprs.insert(expr);
        }
        constant
    }

    fn all(&mut self, exprs: &[ExprId]) -> bool {
        let mut constant = true;
        for expr in exprs {
            constant &= self.expr(*expr);
        }
        constant
    }

    /// The immutable locals `pat` binds, or `None` when it binds a mutable one.
    fn immutable_binds(&self, pat: PatId) -> Option<Vec<LocalId>> {
        let mut out = Vec::new();
        let mut stack = vec![pat];
        while let Some(pat) = stack.pop() {
            match self.store.pat(pat) {
                Pat::Bind { mutable, .. } => {
                    if let Some(local) = self.res.bindings.get(&pat) {
                        if *mutable {
                            return None;
                        }
                        out.push(*local);
                    }
                }
                Pat::Tuple(pats) | Pat::TupleStruct { pats, .. } | Pat::Or(pats) => {
                    stack.extend(pats.iter().copied());
                }
                Pat::Record { fields, .. } => stack.extend(fields.iter().map(|(_, pat)| *pat)),
                Pat::Missing => return None,
                Pat::Wild | Pat::Path(_) | Pat::Lit { .. } | Pat::Range { .. } | Pat::Rest => {}
            }
        }
        Some(out)
    }

    /// Registers the bindings of `pat` matched against the constant `source`; false when
    /// one of them is mutable.
    fn bind_const(&mut self, pat: PatId, source: ExprId) -> bool {
        match self.immutable_binds(pat) {
            Some(locals) => {
                for local in locals {
                    self.out.locals.insert(local, ConstLocal { pat, source });
                }
                true
            }
            None => false,
        }
    }

    fn condition(&mut self, cond: &Condition) -> bool {
        match cond {
            Condition::Expr(expr) => self.expr(*expr),
            Condition::Let { pat, expr } => {
                let constant = self.expr(*expr);
                constant && self.bind_const(*pat, *expr)
            }
        }
    }

    fn expr_inner(&mut self, expr: ExprId) -> bool {
        match self.store.expr(expr) {
            Expr::Missing => false,
            Expr::Literal(_) => true,
            Expr::Path(_) => match self.infer.paths.get(&expr) {
                Some(ValueRef::Local(local)) => self.out.locals.contains_key(local),
                Some(
                    ValueRef::Fn(_)
                    | ValueRef::Ctor(_)
                    | ValueRef::Const { .. }
                    | ValueRef::ConstParam { .. },
                ) => true,
                Some(ValueRef::Static(item)) => matches!(
                    item_tree(self.db, item.file)
                        .item(item.index)
                        .map(|data| &data.kind),
                    Some(ItemKind::Static { mutable: false })
                ),
                Some(ValueRef::Error) | None => false,
            },
            Expr::Block { stmts, tail, .. } => {
                let mut constant = true;
                for stmt in stmts {
                    constant &= match stmt {
                        Stmt::Let { pat, init, .. } => match init {
                            Some(init) => {
                                let init_constant = self.expr(*init);
                                init_constant && self.bind_const(*pat, *init)
                            }
                            None => false,
                        },
                        Stmt::Expr { expr, .. } => self.expr(*expr),
                        Stmt::Item(_) => true,
                    };
                }
                match tail {
                    Some(tail) => self.expr(*tail) && constant,
                    None => constant,
                }
            }
            Expr::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let cond = self.condition(cond);
                let then_branch = self.expr(*then_branch);
                let else_branch = else_branch.is_none_or(|branch| self.expr(branch));
                cond && then_branch && else_branch
            }
            Expr::Match { scrutinee, arms } => {
                let mut constant = self.expr(*scrutinee);
                for arm in arms {
                    if constant {
                        constant &= self.bind_const(arm.pat, *scrutinee);
                    }
                    if let Some(guard) = arm.guard {
                        constant &= self.expr(guard);
                    }
                    constant &= self.expr(arm.body);
                }
                constant
            }
            Expr::Loop { body, .. } => {
                self.expr(*body);
                false
            }
            Expr::While { cond, body, .. } => {
                self.condition(cond);
                self.expr(*body);
                false
            }
            Expr::For { iterable, body, .. } => {
                self.expr(*iterable);
                self.expr(*body);
                false
            }
            Expr::Break { value, .. } | Expr::Return { value } => {
                if let Some(value) = value {
                    self.expr(*value);
                }
                false
            }
            Expr::Continue { .. } => false,
            Expr::Call { callee, args } => {
                let callee_constant = self.expr(*callee);
                let args_constant = self.all(args);
                let const_fn = matches!(
                    self.infer.paths.get(callee),
                    Some(ValueRef::Fn(fn_ref)) if const_callee(self.db, fn_ref.def)
                );
                callee_constant && args_constant && const_fn
            }
            Expr::MethodCall { receiver, args, .. } => {
                let receiver_constant = self.expr(*receiver);
                let args_constant = self.all(args);
                let const_fn = matches!(
                    self.infer.method_calls.get(&expr),
                    Some(call) if const_callee(self.db, call.func.def)
                        && call.receiver.autoref != Some(true)
                );
                receiver_constant && args_constant && const_fn
            }
            Expr::Field { base, .. } | Expr::Cast { expr: base, .. } => self.expr(*base),
            Expr::Unary { expr: inner, .. } => self.expr(*inner),
            Expr::Index { base, index } => {
                let base = self.expr(*base);
                self.expr(*index) && base
            }
            Expr::Try(inner) => {
                self.expr(*inner);
                false
            }
            Expr::Ref {
                mutable,
                expr: inner,
            } => self.expr(*inner) && !mutable,
            Expr::Binary { lhs, rhs, .. } => {
                let lhs = self.expr(*lhs);
                self.expr(*rhs) && lhs
            }
            Expr::Assign { target, value, .. } => {
                self.expr(*target);
                self.expr(*value);
                false
            }
            Expr::Range { start, end, .. } => {
                let start = start.is_some_and(|start| self.expr(start));
                let end = end.is_some_and(|end| self.expr(end));
                start && end
            }
            Expr::Tuple(items) | Expr::Array(ArrayExpr::List(items)) => self.all(items),
            Expr::Array(ArrayExpr::Repeat { value, count }) => {
                let value = self.expr(*value);
                self.expr(*count) && value
            }
            Expr::Record { fields, .. } => {
                let mut constant = true;
                for (_, value) in fields {
                    constant &= self.expr(*value);
                }
                constant
            }
            Expr::Closure { body, .. } => {
                self.expr(*body);
                self.infer
                    .closures
                    .get(&expr)
                    .is_some_and(|info| info.captures.is_empty())
            }
        }
    }
}

/// The direct subexpressions of `expr`, in evaluation order.
pub(crate) fn children(expr: &Expr) -> Vec<ExprId> {
    let mut out = Vec::new();
    let cond = |cond: &Condition, out: &mut Vec<ExprId>| match cond {
        Condition::Expr(expr) | Condition::Let { expr, .. } => out.push(*expr),
    };
    match expr {
        Expr::Missing | Expr::Literal(_) | Expr::Path(_) | Expr::Continue { .. } => {}
        Expr::Block { stmts, tail, .. } => {
            for stmt in stmts {
                match stmt {
                    Stmt::Let { init, .. } => out.extend(init.iter().copied()),
                    Stmt::Expr { expr, .. } => out.push(*expr),
                    Stmt::Item(_) => {}
                }
            }
            out.extend(tail.iter().copied());
        }
        Expr::If {
            cond: condition,
            then_branch,
            else_branch,
        } => {
            cond(condition, &mut out);
            out.push(*then_branch);
            out.extend(else_branch.iter().copied());
        }
        Expr::Match { scrutinee, arms } => {
            out.push(*scrutinee);
            for arm in arms {
                out.extend(arm.guard.iter().copied());
                out.push(arm.body);
            }
        }
        Expr::Loop { body, .. } => out.push(*body),
        Expr::While {
            cond: condition,
            body,
            ..
        } => {
            cond(condition, &mut out);
            out.push(*body);
        }
        Expr::For { iterable, body, .. } => {
            out.push(*iterable);
            out.push(*body);
        }
        Expr::Break { value, .. } | Expr::Return { value } => out.extend(value.iter().copied()),
        Expr::Call { callee, args } => {
            out.push(*callee);
            out.extend(args.iter().copied());
        }
        Expr::MethodCall { receiver, args, .. } => {
            out.push(*receiver);
            out.extend(args.iter().copied());
        }
        Expr::Field { base, .. }
        | Expr::Try(base)
        | Expr::Cast { expr: base, .. }
        | Expr::Unary { expr: base, .. }
        | Expr::Ref { expr: base, .. } => out.push(*base),
        Expr::Index { base, index } => {
            out.push(*base);
            out.push(*index);
        }
        Expr::Binary { lhs, rhs, .. } => {
            out.push(*lhs);
            out.push(*rhs);
        }
        Expr::Assign { target, value, .. } => {
            out.push(*target);
            out.push(*value);
        }
        Expr::Range { start, end, .. } => {
            out.extend(start.iter().copied());
            out.extend(end.iter().copied());
        }
        Expr::Tuple(items) | Expr::Array(ArrayExpr::List(items)) => {
            out.extend(items.iter().copied());
        }
        Expr::Array(ArrayExpr::Repeat { value, count }) => {
            out.push(*value);
            out.push(*count);
        }
        Expr::Record { fields, .. } => out.extend(fields.iter().map(|(_, value)| *value)),
        Expr::Closure { body, .. } => out.push(*body),
    }
    out
}
