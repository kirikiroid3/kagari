//! The walk over one body: context kinds, call rules, scenes and instant procedures,
//! story variables, finite values and second-class closures and references.

use kagari_syntax::TextRange;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::eval::ConstExprs;
use crate::hir::{
    ArrayExpr, Body, ClosureKind, Condition, Expr, ExprId, ExprStore, LangItem, Pat, PatId,
    PathKind, Stmt, TypeRefId,
};
use crate::infer::{Coercion, InferenceResult, ValueRef};
use crate::input::FilePath;
use crate::item_tree::{FnKind, Item, ItemId, ItemKind};
use crate::resolve::{BodyResolution, LocalId};
use crate::signatures::ItemSig;
use crate::ty::{FnDefId, Ty};

use super::{EdgeKind, Env, FileKindChecks, InstantCall, fn_kind_word};

/// The code a body (or closure) belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Ctx {
    pub kind: ClosureKind,
    /// A `#[scene]` `pushdown fn` body (never a closure).
    pub scene: bool,
    pub closure: bool,
}

/// What owns the body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Owner {
    Fn,
    Const,
    Static { mutable: bool },
}

/// Where an expression's value goes, for the second-class closure rule and for skipping
/// assignment targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Pos {
    Value,
    /// A direct argument of a call (the receiver excluded) of a `const fn` or a constructor.
    Arg,
    /// A direct argument of a call of a `pushdown fn` (or a pointer or closure of that
    /// kind): the callee receives it as a parameter.
    ProcArg,
    Callee,
    /// The initializer of a `let`: the local it binds is checked instead.
    LetInit,
    /// An assignment target.
    Place,
}

/// How a pattern binds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Binding {
    /// A `let` statement.
    Let,
    /// A `match` arm, `if let`, `while let` or `for`.
    Pattern,
    /// A closure parameter.
    Param,
}

/// What a call calls, by kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CalleeKind {
    Plain,
    Const,
    Instant,
    Scene,
    /// `push_step`.
    Step,
    /// `rng_choose`.
    Rng,
}

/// A called function: a named one or a value of pointer or closure type.
#[derive(Clone, Debug)]
pub(super) enum Callee {
    Item(ItemId),
    Value(Ty),
}

/// A local bound to a `#[must_use]` value: the binding, the type and whether it is ever
/// the receiver of one of the type's own methods.
pub(super) struct MustUseLocal {
    pub local: LocalId,
    pub pat: PatId,
    pub adt: ItemId,
    pub ty: Ty,
    pub used: bool,
}

pub(super) struct BodyChecker<'a, 'db> {
    pub env: &'a Env<'db>,
    pub path: FilePath,
    pub text: Option<&'a str>,
    pub item: ItemId,
    pub owner: Owner,
    pub store: &'a ExprStore,
    pub res: &'a BodyResolution,
    pub infer: &'a InferenceResult,
    pub consts: &'a ConstExprs,
    pub stack: Vec<Ctx>,
    /// Locals bound as parameters (of the function, `self`, or a closure).
    pub params: FxHashSet<LocalId>,
    /// Greater than zero inside an expression already reported as not finite.
    pub finite_off: u32,
    /// Paths to `Meta` handles reported in `const` code (their method calls are not
    /// reported again).
    pub meta_paths: FxHashSet<ExprId>,
    pub choice_ids: FxHashMap<String, TextRange>,
    pub must_use: Vec<MustUseLocal>,
    /// The item is an instant procedure: its direct calls of instant procedures are
    /// recorded for the recursion check.
    pub instant: bool,
    pub out: &'a mut FileKindChecks,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn check_body(
    env: &Env<'_>,
    item: ItemId,
    data: &Item,
    body: &Body,
    res: &BodyResolution,
    infer: &InferenceResult,
    consts: &ConstExprs,
    out: &mut FileKindChecks,
) {
    let (ctx, owner) = match data.kind {
        ItemKind::Fn { .. } => {
            let Some(sig) = env.sigs.fn_sig(item) else {
                return;
            };
            let kind = match sig.kind {
                FnKind::Plain => ClosureKind::Plain,
                FnKind::Const => ClosureKind::Const,
                FnKind::Pushdown => ClosureKind::Pushdown,
            };
            let ctx = Ctx {
                kind,
                scene: sig.scene && sig.kind == FnKind::Pushdown,
                closure: false,
            };
            (ctx, Owner::Fn)
        }
        ItemKind::Const { .. } => (const_ctx(), Owner::Const),
        ItemKind::Static { mutable } => (const_ctx(), Owner::Static { mutable }),
        _ => return,
    };
    let mut params = FxHashSet::default();
    for (local, data) in res.locals.iter() {
        if data.pat.is_none() {
            params.insert(local);
        }
    }
    let mut stack: Vec<PatId> = body.params.clone();
    while let Some(pat) = stack.pop() {
        if let Some(&local) = res.bindings.get(&pat) {
            params.insert(local);
        }
        push_sub_pats(body.store.pat(pat), &mut stack);
    }
    let mut checker = BodyChecker {
        env,
        path: item.file.path(env.db).clone(),
        text: item.file.text(env.db).as_deref(),
        item,
        owner,
        store: &body.store,
        res,
        infer,
        consts,
        stack: vec![ctx],
        params,
        finite_off: 0,
        meta_paths: FxHashSet::default(),
        choice_ids: FxHashMap::default(),
        must_use: Vec::new(),
        instant: ctx.kind == ClosureKind::Pushdown && !ctx.scene,
        out,
    };
    checker.check_initializer(body.value);
    checker.expr(body.value, Pos::Value);
    checker.finish_must_use();
}

fn const_ctx() -> Ctx {
    Ctx {
        kind: ClosureKind::Const,
        scene: false,
        closure: false,
    }
}

/// The direct sub-patterns of `pat`.
pub(super) fn push_sub_pats(pat: &Pat, out: &mut Vec<PatId>) {
    match pat {
        Pat::Tuple(pats) | Pat::TupleStruct { pats, .. } | Pat::Or(pats) => {
            out.extend(pats.iter().copied());
        }
        Pat::Record { fields, .. } => out.extend(fields.iter().map(|(_, pat)| *pat)),
        Pat::Missing
        | Pat::Wild
        | Pat::Bind { .. }
        | Pat::Path(_)
        | Pat::Lit { .. }
        | Pat::Range { .. }
        | Pat::Rest => {}
    }
}

/// The direct subexpressions of `expr`.
pub(super) fn children(expr: &Expr) -> Vec<ExprId> {
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

impl BodyChecker<'_, '_> {
    pub(super) fn ctx(&self) -> Ctx {
        self.stack.last().copied().unwrap_or_else(const_ctx)
    }

    pub(super) fn range(&self, expr: ExprId) -> TextRange {
        self.store.expr_range(expr)
    }

    pub(super) fn report(&mut self, diag: Diagnostic) {
        self.out.diagnostics.push(diag);
    }

    pub(super) fn diag(
        &self,
        range: TextRange,
        code: DiagnosticCode,
        message: impl Into<String>,
    ) -> Diagnostic {
        Diagnostic::new(self.path.clone(), range, code, message)
    }

    /// The type of `expr` before coercion; `None` when unknown or erroneous.
    pub(super) fn ty(&self, expr: ExprId) -> Option<&Ty> {
        self.infer
            .expr_types
            .get(expr)
            .filter(|ty| !ty.references_error())
    }

    /// The type of the value `expr` produces: after a coercion to a pointer or a closure.
    fn value_ty(&self, expr: ExprId) -> Option<&Ty> {
        match self.infer.coercions.get(&expr) {
            Some((Coercion::FnDefToPtr | Coercion::ToClosure | Coercion::ClosureToPtr, ty))
                if !ty.references_error() =>
            {
                Some(ty)
            }
            _ => self.ty(expr),
        }
    }

    /// The source text of `expr`.
    pub(super) fn src(&self, expr: ExprId) -> String {
        let range = self.range(expr);
        self.text
            .and_then(|text| text.get(usize::from(range.start())..usize::from(range.end())))
            .map_or_else(|| "this value".to_owned(), ToOwned::to_owned)
    }

    pub(super) fn local_name(&self, local: LocalId) -> String {
        self.res.locals[local].name.to_string()
    }

    /// The code the current context is, for messages: "the scene `Day1`", "a closure".
    pub(super) fn ctx_desc(&self) -> String {
        let ctx = self.ctx();
        if ctx.closure {
            return match ctx.kind {
                ClosureKind::Plain => "a closure",
                ClosureKind::Const => "a `const` closure",
                ClosureKind::Pushdown => "a `pushdown` closure",
            }
            .to_owned();
        }
        let name = self.env.fn_path(self.item);
        match self.owner {
            Owner::Const => format!("the initializer of the constant `{name}`"),
            Owner::Static { mutable: true } => {
                format!("the initializer of the story variable `{name}`")
            }
            Owner::Static { mutable: false } => format!("the initializer of the static `{name}`"),
            Owner::Fn => match ctx.kind {
                ClosureKind::Plain => format!("the `fn` `{name}`"),
                ClosureKind::Const => format!("the `const fn` `{name}`"),
                ClosureKind::Pushdown if ctx.scene => format!("the scene `{name}`"),
                ClosureKind::Pushdown => format!("the instant procedure `{name}`"),
            },
        }
    }

    // ---- the walk ----

    /// Visits `expr` and everything in it; returns whether `expr` itself was reported by
    /// the finite-value or second-class closure rule.
    pub(super) fn expr(&mut self, expr: ExprId, pos: Pos) -> bool {
        let reported = self.ctx().kind == ClosureKind::Pushdown && self.check_value(expr, pos);
        if reported {
            self.finite_off = self.finite_off.saturating_add(1);
        }
        self.expr_inner(expr, pos);
        if reported {
            self.finite_off = self.finite_off.saturating_sub(1);
        }
        reported
    }

    fn expr_inner(&mut self, expr: ExprId, pos: Pos) {
        let store = self.store;
        match store.expr(expr) {
            Expr::Missing | Expr::Literal(_) | Expr::Continue { .. } => {}
            Expr::Path(_) => self.path_expr(expr, pos),
            Expr::Block { stmts, tail, .. } => {
                for stmt in stmts {
                    self.stmt(stmt);
                }
                if let Some(tail) = tail {
                    self.expr(*tail, Pos::Value);
                }
            }
            Expr::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.condition(cond);
                self.expr(*then_branch, Pos::Value);
                if let Some(else_branch) = else_branch {
                    self.expr(*else_branch, Pos::Value);
                }
            }
            Expr::Match { scrutinee, arms } => {
                self.expr(*scrutinee, Pos::Value);
                for arm in arms {
                    self.bind_pat(arm.pat, Binding::Pattern, false);
                    if let Some(guard) = arm.guard {
                        self.expr(guard, Pos::Value);
                    }
                    self.expr(arm.body, Pos::Value);
                }
            }
            Expr::Loop { body, .. } => {
                self.check_loop(expr, "loop");
                self.expr(*body, Pos::Value);
            }
            Expr::While { cond, body, .. } => {
                let keyword = match cond {
                    Condition::Expr(_) => "while",
                    Condition::Let { .. } => "while let",
                };
                self.check_loop(expr, keyword);
                self.condition(cond);
                self.expr(*body, Pos::Value);
            }
            Expr::For {
                pat,
                iterable,
                body,
                ..
            } => {
                self.check_for(*iterable);
                self.expr(*iterable, Pos::Value);
                self.bind_pat(*pat, Binding::Pattern, false);
                self.expr(*body, Pos::Value);
            }
            Expr::Break { value, .. } | Expr::Return { value } => {
                if let Some(value) = value {
                    self.expr(*value, Pos::Value);
                }
            }
            Expr::Call { callee, args } => self.call(expr, *callee, args),
            Expr::MethodCall { receiver, args, .. } => self.method_call(expr, *receiver, args),
            Expr::Field { base, .. }
            | Expr::Try(base)
            | Expr::Cast { expr: base, .. }
            | Expr::Unary { expr: base, .. } => {
                self.expr(*base, Pos::Value);
            }
            Expr::Index { base, index } => {
                self.expr(*base, Pos::Value);
                self.expr(*index, Pos::Value);
            }
            Expr::Ref {
                mutable,
                expr: inner,
            } => {
                if *mutable {
                    self.check_mut_place(*inner, super::places::Access::Borrow);
                }
                self.expr(*inner, Pos::Value);
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.expr(*lhs, Pos::Value);
                self.expr(*rhs, Pos::Value);
            }
            Expr::Assign { target, value, .. } => {
                self.check_mut_place(*target, super::places::Access::Assign);
                self.note_stored_pointer(*value);
                self.expr(*target, Pos::Place);
                self.expr(*value, Pos::Value);
            }
            Expr::Range { start, end, .. } => {
                for bound in [start, end].into_iter().flatten() {
                    self.expr(*bound, Pos::Value);
                }
            }
            Expr::Tuple(items) | Expr::Array(ArrayExpr::List(items)) => {
                for item in items {
                    self.expr(*item, Pos::Value);
                }
            }
            Expr::Array(ArrayExpr::Repeat { value, count }) => {
                self.expr(*value, Pos::Value);
                self.expr(*count, Pos::Value);
            }
            Expr::Record { fields, .. } => {
                self.record(expr, fields);
                for (_, value) in fields {
                    self.expr(*value, Pos::Value);
                }
            }
            Expr::Closure { params, body, .. } => self.closure(expr, params, *body),
        }
    }

    fn stmt(&mut self, stmt: &Stmt) {
        match *stmt {
            Stmt::Let { pat, init, .. } => {
                let init_reported = init.is_some_and(|init| self.expr(init, Pos::LetInit));
                self.bind_pat(pat, Binding::Let, init_reported);
            }
            Stmt::Expr { expr, .. } => {
                self.expr(expr, Pos::Value);
                self.must_use_stmt(expr);
            }
            Stmt::Item(_) => {}
        }
    }

    fn condition(&mut self, cond: &Condition) {
        match *cond {
            Condition::Expr(expr) => {
                self.expr(expr, Pos::Value);
            }
            Condition::Let { pat, expr } => {
                self.expr(expr, Pos::Value);
                self.bind_pat(pat, Binding::Pattern, false);
            }
        }
    }

    /// Checks the locals `pat` binds. `skip_finite`: the initializer was already reported.
    fn bind_pat(&mut self, pat: PatId, binding: Binding, skip_finite: bool) {
        if binding == Binding::Let
            && matches!(self.store.pat(pat), Pat::Bind { .. })
            && let Some(&local) = self.res.bindings.get(&pat)
        {
            self.register_must_use(local, pat);
        }
        let mut stack = vec![pat];
        while let Some(pat) = stack.pop() {
            let data = self.store.pat(pat);
            if let Pat::Bind { .. } = data
                && let Some(&local) = self.res.bindings.get(&pat)
            {
                if binding == Binding::Param {
                    self.params.insert(local);
                }
                self.check_local(local, pat, binding, skip_finite);
            }
            push_sub_pats(data, &mut stack);
        }
    }

    /// In the story: a local holds a finite value, never a reference or a closure, except
    /// as a parameter.
    fn check_local(&mut self, local: LocalId, pat: PatId, binding: Binding, skip_finite: bool) {
        if self.ctx().kind != ClosureKind::Pushdown || self.consts.locals.contains_key(&local) {
            return;
        }
        let Some(ty) = self.infer.local_types.get(local) else {
            return;
        };
        if ty.references_error() {
            return;
        }
        let range = self.store.pat_range(pat);
        let name = self.local_name(local);
        let ctx = self.ctx_desc();
        let diag = match ty {
            Ty::Ref { inner, .. } if binding == Binding::Param => {
                if self.env.is_finite(inner, false) {
                    return;
                }
                self.not_finite_local(range, &name, ty)
            }
            Ty::Ref { .. } => self.diag(
                range,
                DiagnosticCode::KindSecondClassReference,
                format!(
                    "`{name}` cannot hold a reference in {ctx}: references in the story are \
                     second-class and can only be passed as arguments"
                ),
            ),
            Ty::Closure { .. } if binding == Binding::Param => return,
            Ty::Closure { .. } => self.diag(
                range,
                DiagnosticCode::KindSecondClassClosure,
                format!(
                    "`{name}` cannot hold a closure in {ctx}: closures in the story are \
                     second-class and can only be passed as arguments and called"
                ),
            ),
            _ if skip_finite || self.env.is_finite(ty, false) => return,
            _ => self.not_finite_local(range, &name, ty),
        };
        self.report(diag);
    }

    fn not_finite_local(&self, range: TextRange, name: &str, ty: &Ty) -> Diagnostic {
        self.diag(
            range,
            DiagnosticCode::KindNotFinite,
            format!(
                "`{name}` has type `{}`, which is not finite: the story can hold only finite \
                 types at runtime",
                self.env.show(ty)
            ),
        )
    }

    /// Whether `expr` is a local or a field, element or dereference of one: its finiteness
    /// follows from the local's, which is checked where it is bound.
    fn rooted_at_local(&self, mut expr: ExprId) -> bool {
        loop {
            match self.store.expr(expr) {
                Expr::Path(_) => {
                    return matches!(self.infer.paths.get(&expr), Some(ValueRef::Local(_)));
                }
                Expr::Field { base, .. }
                | Expr::Index { base, .. }
                | Expr::Unary {
                    op: crate::hir::UnaryOp::Deref,
                    expr: base,
                } => expr = *base,
                _ => return false,
            }
        }
    }

    /// The finite-value rule and the second-class closure rule for one expression of the
    /// story; returns whether it reported.
    fn check_value(&mut self, expr: ExprId, pos: Pos) -> bool {
        if self.finite_off > 0 || matches!(self.store.expr(expr), Expr::Missing) {
            return false;
        }
        let Some(ty) = self.value_ty(expr) else {
            return false;
        };
        if let Ty::Closure { .. } = ty {
            if matches!(pos, Pos::Arg | Pos::ProcArg | Pos::Callee | Pos::LetInit) {
                return false;
            }
            let diag = self.diag(
                self.range(expr),
                DiagnosticCode::KindSecondClassClosure,
                format!(
                    "closures in the story are second-class: in {} a closure can only be \
                     passed as an argument or called, never stored, returned or captured",
                    self.ctx_desc()
                ),
            );
            self.report(diag);
            return true;
        }
        let call_kind = self.call_kind(expr);
        // A call of a `fn` from the story is already a call-rule error.
        if pos == Pos::Place
            || call_kind == Some(CalleeKind::Plain)
            || self.consts.contains(expr)
            || self.rooted_at_local(expr)
            || self.env.is_finite(ty, true)
        {
            return false;
        }
        let shown = self.env.show(ty);
        let (code, message) = if self.env.contains_closure(ty) {
            (
                DiagnosticCode::KindSecondClassClosure,
                format!(
                    "this value of type `{shown}` holds a closure: closures in the story are \
                     second-class and can only be passed as arguments and called, never stored"
                ),
            )
        } else if self.env.contains_ref(ty) && !matches!(ty, Ty::Ref { .. }) {
            (
                DiagnosticCode::KindSecondClassReference,
                format!(
                    "this value of type `{shown}` holds a reference: references in the story are \
                     second-class and can only be passed as arguments, never stored"
                ),
            )
        } else if call_kind == Some(CalleeKind::Const) {
            (
                DiagnosticCode::KindNotFinite,
                format!(
                    "this `const fn` call has runtime arguments, so it is tabulated, and its \
                     result must be finite, but `{shown}` is not"
                ),
            )
        } else {
            (
                DiagnosticCode::KindNotFinite,
                format!(
                    "this expression has type `{shown}`, which is not finite: the story can \
                     hold only finite types at runtime (constant expressions are folded)"
                ),
            )
        };
        let diag = self.diag(self.range(expr), code, message);
        self.report(diag);
        true
    }

    /// The kind of the function a call or method call calls.
    fn call_kind(&self, expr: ExprId) -> Option<CalleeKind> {
        let callee = match self.store.expr(expr) {
            Expr::Call { callee, .. } => self.ty(*callee).and_then(|ty| self.callee_of_ty(ty)),
            Expr::MethodCall { .. } => {
                self.infer
                    .method_calls
                    .get(&expr)
                    .and_then(|call| match call.func.def {
                        FnDefId::Item(item) => Some(Callee::Item(item)),
                        _ => None,
                    })
            }
            _ => None,
        };
        self.callee_kind(&callee?)
    }

    fn path_expr(&mut self, expr: ExprId, pos: Pos) {
        if pos != Pos::Callee
            && let Some(ValueRef::Fn(fn_ref)) = self.infer.paths.get(&expr)
            && let FnDefId::Item(item) = fn_ref.def
            && self.callee_kind(&Callee::Item(item)) == Some(CalleeKind::Instant)
        {
            // An instant procedure used as a value: whoever receives it may call it.
            self.out.instant_values.push((item, pos != Pos::ProcArg));
            self.record_instant_call(expr, item, EdgeKind::Value);
            return;
        }
        let Some(ValueRef::Static(item)) = self.infer.paths.get(&expr) else {
            return;
        };
        let item = *item;
        let Some(ItemSig::Static { ty, mutable }) = self.env.sigs.get(item) else {
            return;
        };
        let ctx = self.ctx();
        let name = self.env.item_name(item);
        if *mutable && ctx.kind != ClosureKind::Pushdown {
            let diag = self.diag(
                self.range(expr),
                DiagnosticCode::KindStoryVariableOutsideStory,
                format!(
                    "story variable `{name}` cannot be used in {}: only `pushdown fn` code may \
                     read or write story variables",
                    self.ctx_desc()
                ),
            );
            let diag = self.env.declared_here(diag, item);
            self.report(diag);
        } else if !*mutable && ctx.kind == ClosureKind::Const && self.env.is_meta(ty) {
            self.meta_paths.insert(expr);
            let diag = self.diag(
                self.range(expr),
                DiagnosticCode::KindMetaInConst,
                format!(
                    "meta field `{name}` cannot be used in {}: meta is read by `fn` code and \
                     written by the story, never seen by `const` code",
                    self.ctx_desc()
                ),
            );
            self.report(diag);
        }
    }

    fn closure(&mut self, expr: ExprId, params: &[(PatId, Option<TypeRefId>)], body: ExprId) {
        let outer = self.ctx();
        let info = self.infer.closures.get(&expr);
        if outer.kind == ClosureKind::Pushdown
            && let Some(info) = info
            && let Some(&captured) = info.captures.iter().find(|&&local| {
                matches!(self.infer.local_types.get(local), Some(Ty::Closure { .. }))
            })
        {
            let name = self.local_name(captured);
            let diag = self.diag(
                self.range(expr),
                DiagnosticCode::KindSecondClassClosure,
                format!(
                    "this closure captures the closure `{name}`: closures in the story are \
                     second-class and cannot be captured"
                ),
            );
            self.report(diag);
        }
        let kind = info.map_or(outer.kind, |info| info.kind);
        self.stack.push(Ctx {
            kind,
            scene: false,
            closure: true,
        });
        let saved = std::mem::take(&mut self.finite_off);
        for (pat, _) in params {
            self.bind_pat(*pat, Binding::Param, false);
        }
        self.expr(body, Pos::Value);
        self.finite_off = saved;
        self.stack.pop();
    }

    /// `loop` and `while` are unbounded: not in an instant procedure.
    fn check_loop(&mut self, expr: ExprId, keyword: &str) {
        let ctx = self.ctx();
        if ctx.kind != ClosureKind::Pushdown || ctx.scene {
            return;
        }
        let why = if ctx.closure {
            "closures in the story are instant procedures, which have"
        } else {
            "an instant procedure has"
        };
        let diag = self.diag(
            self.range(expr),
            DiagnosticCode::KindUnboundedLoop,
            format!(
                "`{keyword}` is not allowed in {}: {why} only bounded loops (`for` over an \
                 array or a range with constant bounds)",
                self.ctx_desc()
            ),
        );
        self.report(diag);
    }

    /// `for` in the story: over an array, or a range whose bounds are constant.
    fn check_for(&mut self, iterable: ExprId) {
        if self.ctx().kind != ClosureKind::Pushdown {
            return;
        }
        let Some(mut ty) = self.ty(iterable) else {
            return;
        };
        while let Ty::Ref { inner, .. } = ty {
            ty = inner;
        }
        let constant_range = matches!(
            self.store.expr(iterable),
            Expr::Range { start: Some(start), end: Some(end), .. }
                if self.consts.contains(*start) && self.consts.contains(*end)
        );
        if matches!(ty, Ty::Array { .. }) || constant_range || self.consts.contains(iterable) {
            return;
        }
        let diag = self.diag(
            self.range(iterable),
            DiagnosticCode::KindForNotConstant,
            format!(
                "a `for` loop in {} must iterate over an array or over a range whose bounds \
                 are constant expressions",
                self.ctx_desc()
            ),
        );
        self.report(diag);
    }

    // ---- calls ----

    fn call(&mut self, expr: ExprId, callee: ExprId, args: &[ExprId]) {
        let direct = match self.infer.paths.get(&callee) {
            Some(ValueRef::Fn(fn_ref)) => match fn_ref.def {
                FnDefId::Item(item) => Some(item),
                _ => None,
            },
            _ => None,
        };
        let std = &self.env.sigs.std;
        // A non-constant text effect is reported once, not again as a non-finite value.
        let bad_effect = match direct {
            Some(item) if Some(item) == std.effect => self.effect(args),
            _ => None,
        };
        if direct.is_some() && direct == std.mono_ruby {
            self.mono_ruby(expr, args);
        }
        let target = match direct {
            Some(item) => Some(Callee::Item(item)),
            None => self.ty(callee).and_then(|ty| self.callee_of_ty(ty)),
        };
        let arg_pos = self.arg_pos(target.as_ref());
        self.expr(callee, Pos::Callee);
        for &arg in args {
            let skip = bad_effect == Some(arg);
            if skip {
                self.finite_off = self.finite_off.saturating_add(1);
            }
            self.expr(arg, arg_pos);
            if skip {
                self.finite_off = self.finite_off.saturating_sub(1);
            }
        }
        if let Some(target) = &target {
            let from_form = matches!(
                self.store.expr(callee),
                Expr::Path(path) if path.kind == PathKind::Lang(LangItem::PushStep)
            );
            self.check_call(expr, target, from_form);
        }
        match (direct, &target) {
            (Some(item), _) => self.record_instant_call(expr, item, EdgeKind::Direct),
            (None, Some(target)) => {
                if self.instant
                    && self.ctx().kind == ClosureKind::Pushdown
                    && self.callee_kind(target) == Some(CalleeKind::Instant)
                {
                    self.out.indirect_calls.push((self.item, self.range(expr)));
                }
            }
            (None, None) => {}
        }
        if self.ctx().kind == ClosureKind::Pushdown {
            let args: Vec<_> = args
                .iter()
                .map(|&arg| super::places::CallArg::Value(arg))
                .collect();
            self.check_exclusive(&args);
        }
    }

    fn method_call(&mut self, expr: ExprId, receiver: ExprId, args: &[ExprId]) {
        self.expr(receiver, Pos::Value);
        let method = match self.infer.method_calls.get(&expr).map(|call| call.func.def) {
            Some(FnDefId::Item(method)) => Some(Callee::Item(method)),
            _ => None,
        };
        let arg_pos = self.arg_pos(method.as_ref());
        for &arg in args {
            self.expr(arg, arg_pos);
        }
        let Some(call) = self.infer.method_calls.get(&expr) else {
            return;
        };
        let FnDefId::Item(method) = call.func.def else {
            return;
        };
        if !self.meta_paths.contains(&receiver) {
            self.check_call(expr, &Callee::Item(method), false);
        }
        if call.receiver.autoref == Some(true) {
            self.check_receiver_mut(receiver, call.receiver.derefs);
        }
        self.must_use_receiver(receiver, method);
        self.record_instant_call(expr, method, EdgeKind::Direct);
        if self.ctx().kind == ClosureKind::Pushdown {
            let mut all = vec![super::places::CallArg::Receiver(receiver, call.receiver)];
            all.extend(args.iter().map(|&arg| super::places::CallArg::Value(arg)));
            self.check_exclusive(&all);
        }
    }

    pub(super) fn callee_of_ty(&self, ty: &Ty) -> Option<Callee> {
        match ty {
            Ty::FnDef(fn_ref) => match fn_ref.def {
                FnDefId::Item(item) => Some(Callee::Item(item)),
                FnDefId::StructCtor(_) | FnDefId::VariantCtor(..) => None,
            },
            Ty::FnPtr { .. } | Ty::Closure { .. } => Some(Callee::Value(ty.clone())),
            _ => None,
        }
    }

    pub(super) fn callee_kind(&self, callee: &Callee) -> Option<CalleeKind> {
        match callee {
            Callee::Item(item) => {
                let sig = self.env.sigs.fn_sig(*item)?;
                Some(match sig.kind {
                    FnKind::Plain => CalleeKind::Plain,
                    FnKind::Const => CalleeKind::Const,
                    FnKind::Pushdown if Some(*item) == self.env.sigs.std.push_step => {
                        CalleeKind::Step
                    }
                    FnKind::Pushdown if Some(*item) == self.env.sigs.std.rng_choose => {
                        CalleeKind::Rng
                    }
                    FnKind::Pushdown if sig.scene => CalleeKind::Scene,
                    FnKind::Pushdown => CalleeKind::Instant,
                })
            }
            Callee::Value(Ty::FnPtr { kind, .. }) => Some(match kind {
                crate::hir::FnPtrKind::Plain => CalleeKind::Plain,
                crate::hir::FnPtrKind::Const => CalleeKind::Const,
                crate::hir::FnPtrKind::Pushdown => CalleeKind::Instant,
                crate::hir::FnPtrKind::Scene => CalleeKind::Scene,
            }),
            Callee::Value(Ty::Closure { kind, .. }) => Some(match kind {
                ClosureKind::Plain => CalleeKind::Plain,
                ClosureKind::Const => CalleeKind::Const,
                ClosureKind::Pushdown => CalleeKind::Instant,
            }),
            Callee::Value(_) => None,
        }
    }

    fn callee_desc(&self, callee: &Callee, kind: CalleeKind) -> String {
        match callee {
            Callee::Item(item) => {
                let path = self.env.fn_path(*item);
                match kind {
                    CalleeKind::Scene => format!("the scene `{path}`"),
                    CalleeKind::Plain => format!("`{}` `{path}`", fn_kind_word(FnKind::Plain)),
                    CalleeKind::Const => format!("`{}` `{path}`", fn_kind_word(FnKind::Const)),
                    CalleeKind::Instant | CalleeKind::Step | CalleeKind::Rng => {
                        format!("`{}` `{path}`", fn_kind_word(FnKind::Pushdown))
                    }
                }
            }
            Callee::Value(ty) => format!("a value of type `{}`", self.env.show(ty)),
        }
    }

    /// The call rules (kinds.md#call-rules) and the scene rules.
    fn check_call(&mut self, expr: ExprId, callee: &Callee, from_form: bool) {
        let Some(kind) = self.callee_kind(callee) else {
            return;
        };
        let ctx = self.ctx();
        let range = self.range(expr);
        let desc = self.callee_desc(callee, kind);
        let here = self.ctx_desc();
        let (code, message) = match (ctx.kind, kind) {
            (
                ClosureKind::Plain,
                CalleeKind::Instant | CalleeKind::Scene | CalleeKind::Step | CalleeKind::Rng,
            ) => (
                DiagnosticCode::KindCallRule,
                format!(
                    "cannot call {desc} from {here}: the only way into the story is \
                     `nav::start` of an entry scene"
                ),
            ),
            (
                ClosureKind::Const,
                CalleeKind::Plain
                | CalleeKind::Instant
                | CalleeKind::Scene
                | CalleeKind::Step
                | CalleeKind::Rng,
            ) => {
                let mut message = format!(
                    "cannot call {desc} from {here}: `const` code is pure and can call only \
                     `const fn`"
                );
                if self.is_setting_method(callee) {
                    message.push_str(
                        "; in `const` code a `Setting` handle can only be passed as a value, \
                         as to `Entity::gate`",
                    );
                }
                (DiagnosticCode::KindCallRule, message)
            }
            (ClosureKind::Pushdown, CalleeKind::Plain) => (
                DiagnosticCode::KindCallRule,
                format!(
                    "cannot call {desc} from {here}: the story can call only `pushdown fn` and \
                     `const fn`"
                ),
            ),
            (ClosureKind::Pushdown, CalleeKind::Step) if !ctx.scene => (
                DiagnosticCode::KindStepOutsideScene,
                if from_form {
                    format!(
                        "only a scene can run a step: {here} cannot use `s!` or `choice!`; \
                         mark the function `#[scene]`"
                    )
                } else {
                    format!("only a scene can run a step: {here} cannot call `push_step`")
                },
            ),
            (ClosureKind::Pushdown, CalleeKind::Rng) if !ctx.scene => (
                DiagnosticCode::KindRngOutsideScene,
                format!("only a scene can draw a random value: {here} cannot call `rng_choose`"),
            ),
            (ClosureKind::Pushdown, CalleeKind::Scene) if !ctx.scene => (
                DiagnosticCode::KindSceneCallOutsideScene,
                format!("only a scene can call another scene: {here} cannot call {desc}"),
            ),
            _ => return,
        };
        let diag = self.diag(range, code, message);
        let diag = match callee {
            Callee::Item(item) if !from_form => self.env.declared_here(diag, *item),
            _ => diag,
        };
        self.report(diag);
    }

    /// A method of `Setting<T>`.
    fn is_setting_method(&self, callee: &Callee) -> bool {
        let Callee::Item(item) = callee else {
            return false;
        };
        let Some(parent) = crate::signatures::assoc_parent(self.env.db, *item) else {
            return false;
        };
        matches!(
            self.env.sigs.get(parent),
            Some(ItemSig::Impl { self_ty: Ty::Adt { item, .. }, .. })
                if Some(*item) == self.env.sigs.std.setting
        )
    }

    /// Records an edge from this instant procedure to the instant procedure `callee`: a
    /// direct call in `pushdown` code, or a use of `callee` as a value anywhere in the body.
    fn record_instant_call(&mut self, expr: ExprId, callee: ItemId, kind: EdgeKind) {
        if !self.instant || (kind == EdgeKind::Direct && self.ctx().kind != ClosureKind::Pushdown) {
            return;
        }
        if self.callee_kind(&Callee::Item(callee)) != Some(CalleeKind::Instant) {
            return;
        }
        self.out.instant_calls.push(InstantCall {
            caller: self.item,
            callee,
            range: self.range(expr),
            kind,
        });
    }

    /// Where the arguments of a call of `target` go: to a parameter of story code, or to a
    /// `const fn` or constructor that may fold them into a constant.
    fn arg_pos(&self, target: Option<&Callee>) -> Pos {
        match target.and_then(|target| self.callee_kind(target)) {
            Some(CalleeKind::Instant | CalleeKind::Scene | CalleeKind::Step | CalleeKind::Rng) => {
                Pos::ProcArg
            }
            _ => Pos::Arg,
        }
    }

    /// An assignment that stores a runtime `pushdown fn` pointer: any instant procedure
    /// passed around may then come back out of storage (see `instant_recursion`).
    fn note_stored_pointer(&mut self, value: ExprId) {
        if !self.consts.contains(value)
            && self
                .value_ty(value)
                .is_some_and(|ty| self.env.contains_instant_pointer(ty))
        {
            self.out.stores_pointers = true;
        }
    }

    // ---- constant initializers ----

    /// Reports the first non-constant part of a `const` or `static` initializer, unless
    /// another rule reports it (calls of non-`const` functions, story variables).
    pub(super) fn check_initializer(&mut self, value: ExprId) {
        let what = match self.owner {
            Owner::Const => "a `const`",
            Owner::Static { mutable: true } => "a story variable",
            Owner::Static { mutable: false } => "a `static`",
            Owner::Fn => return,
        };
        let Some((expr, reason)) = self.non_const_root(value) else {
            return;
        };
        let diag = self.diag(
            self.range(expr),
            DiagnosticCode::KindNotConstant,
            format!("the initializer of {what} must be a constant expression: {reason}"),
        );
        self.report(diag);
    }

    /// The first innermost non-constant part of `expr` and why it is not constant.
    fn non_const_root(&self, expr: ExprId) -> Option<(ExprId, String)> {
        let store = self.store;
        let data = store.expr(expr);
        if self.consts.contains(expr) || matches!(data, Expr::Missing) || self.ty(expr).is_none() {
            return None;
        }
        let reason = match data {
            Expr::Loop { .. } | Expr::While { .. } | Expr::For { .. } => {
                Some("a loop is not a constant expression".to_owned())
            }
            Expr::Assign { .. } => Some("an assignment is not a constant expression".to_owned()),
            Expr::Ref { mutable: true, .. } => {
                Some("a mutable borrow is not a constant expression".to_owned())
            }
            Expr::Break { .. } | Expr::Continue { .. } | Expr::Return { .. } => {
                Some("`break`, `continue` and `return` are not constant expressions".to_owned())
            }
            Expr::Try(_) => Some("`?` is not a constant expression".to_owned()),
            Expr::Closure { .. } => {
                Some("a closure that captures variables is not a constant expression".to_owned())
            }
            _ => None,
        };
        if let Some(reason) = reason {
            return Some((expr, reason));
        }
        let non_const: Vec<ExprId> = children(data)
            .into_iter()
            .filter(|&child| {
                !self.consts.contains(child)
                    && !matches!(store.expr(child), Expr::Missing)
                    && self.ty(child).is_some()
            })
            .collect();
        if !non_const.is_empty() {
            return non_const
                .into_iter()
                .find_map(|child| self.non_const_root(child));
        }
        let reason = match data {
            Expr::Call { callee, .. } => {
                let target = self.ty(*callee).and_then(|ty| self.callee_of_ty(ty))?;
                if self.callee_kind(&target) != Some(CalleeKind::Const) {
                    return None;
                }
                format!("`{}` is not a constant expression", self.src(expr))
            }
            Expr::MethodCall { receiver, .. } => {
                let call = self.infer.method_calls.get(&expr)?;
                let FnDefId::Item(method) = call.func.def else {
                    return None;
                };
                if self.callee_kind(&Callee::Item(method)) != Some(CalleeKind::Const) {
                    return None;
                }
                if call.receiver.autoref == Some(true) {
                    format!("the call borrows `{}` mutably", self.src(*receiver))
                } else {
                    format!("`{}` is not a constant expression", self.src(expr))
                }
            }
            Expr::Path(_) => match self.infer.paths.get(&expr)? {
                ValueRef::Local(local) if self.res.locals[*local].mutable => {
                    format!("`{}` is mutable", self.local_name(*local))
                }
                ValueRef::Local(local) => format!(
                    "`{}` is not bound to a constant expression",
                    self.local_name(*local)
                ),
                _ => return None,
            },
            _ => format!("`{}` is not a constant expression", self.src(expr)),
        };
        Some((expr, reason))
    }
}
