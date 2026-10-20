//! The body checker shared by fn bodies, const initializers, `render` and inline `<r>`:
//! bidirectional inference over a union-find table of type variables.

use kagari_syntax::TextRange;
use kagari_syntax::represent::ast::{self, BinOp, LiteralKind, PrefixOp};
use kagari_syntax::script::ast::SourceMap;
use rowan::ast::AstNode;
use rustc_hash::FxHashMap;
use smol_str::SmolStr;

use crate::diagnostic::{Diagnostic, DiagnosticCode, Span};
use crate::exhaustive::{self, TyKind, Types, Witness};
use crate::represent::item_tree::{DefId, ItemKind};
use crate::represent::scope::ValueDef;
use crate::represent::ty::{IntTy, Ty, TyCtx, TyEnv, TypeRes, subst};

/// A reference from one definition to another, for recursion and const-cycle checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefEdge {
    pub from: DefId,
    pub to: DefId,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VarKind {
    General,
    /// An unsuffixed integer literal: only integer types unify with it.
    IntLiteral,
}

#[derive(Clone, Debug)]
enum Slot {
    Unbound(VarKind),
    Bound(Ty),
}

/// What a value path names.
enum ValueRes {
    Local(Ty),
    /// A fn, const, asset import or unit struct. `impl_args` are the arguments of the type the
    /// path went through (`Type::item`); `fn_args` the turbofish of the last segment.
    Def {
        def: DefId,
        impl_args: Option<Vec<Ty>>,
        fn_args: Option<Vec<Ty>>,
    },
    Variant {
        enum_def: DefId,
        index: u32,
        args: Vec<Ty>,
    },
}

/// A `match` whose exhaustiveness is checked once the body's types are known.
struct PendingMatch {
    scrutinee: Ty,
    rows: Vec<exhaustive::Pat>,
    range: TextRange,
}

/// Checks one body. Create with [`Body::new`], drive with the `check_*` methods, then
/// [`Body::finish`].
pub struct Body<'a, 'db> {
    tcx: &'a TyCtx<'db>,
    env: TyEnv,
    owner: Option<DefId>,
    /// The fns of the current `.kirp` file, visible by bare name.
    kirp_fns: &'a FxHashMap<SmolStr, DefId>,
    /// Maps body ranges to source ranges (inline `<r>` code).
    map: Option<&'a SourceMap>,
    self_local: bool,
    slots: Vec<Slot>,
    locals: Vec<(SmolStr, Ty)>,
    literals: Vec<(Ty, i128, TextRange)>,
    matches: Vec<PendingMatch>,
    pat_error: bool,
    pub diagnostics: Vec<Diagnostic>,
    pub calls: Vec<RefEdge>,
    pub const_refs: Vec<RefEdge>,
}

impl<'a, 'db> Body<'a, 'db> {
    pub fn new(
        tcx: &'a TyCtx<'db>,
        env: TyEnv,
        owner: Option<DefId>,
        kirp_fns: &'a FxHashMap<SmolStr, DefId>,
        map: Option<&'a SourceMap>,
    ) -> Body<'a, 'db> {
        Body {
            tcx,
            env,
            owner,
            kirp_fns,
            map,
            self_local: false,
            slots: Vec::new(),
            locals: Vec::new(),
            literals: Vec::new(),
            matches: Vec::new(),
            pat_error: false,
            diagnostics: Vec::new(),
            calls: Vec::new(),
            const_refs: Vec::new(),
        }
    }

    // ---- entry points ---------------------------------------------------------------------

    /// Check the body of the fn `def` against its signature.
    pub fn check_fn(&mut self, def: DefId, function: &ast::Fn) {
        let sig = self.tcx.fn_sig(def);
        if sig.has_self {
            self.self_local = true;
            let self_ty = sig.self_ty.clone().unwrap_or(Ty::Error);
            self.bind(SmolStr::new("self"), self_ty);
            if sig.impl_def.is_none()
                && let Some(self_param) = function.param_list().and_then(|list| list.self_param())
            {
                self.error(
                    self_param.syntax().text_range(),
                    DiagnosticCode::TypeSelfOutsideImpl,
                    "a `self` parameter is only allowed on methods inside an impl".to_owned(),
                );
            }
        }
        let params: Vec<ast::Param> = function
            .param_list()
            .map(|list| list.params().collect())
            .unwrap_or_default();
        for (param, ty) in params.iter().zip(sig.params.iter()) {
            match param.pat() {
                Some(ast::Pat::IdentPat(pat)) => {
                    if let Some(name) = pat.name() {
                        self.bind(SmolStr::new(name.text()), ty.clone());
                    }
                }
                Some(ast::Pat::WildcardPat(_)) | None => {}
                Some(other) => {
                    let range = other.syntax().text_range();
                    self.error(
                        range,
                        DiagnosticCode::TypeBadPattern,
                        "parameters must be a name or `_`".to_owned(),
                    );
                }
            }
        }
        if let Some(body) = function.body() {
            let expr = ast::Expr::BlockExpr(body);
            self.check_expr(&expr, &sig.ret);
        }
    }

    /// Check a const initializer against the const's declared type.
    pub fn check_const(&mut self, def: DefId, konst: &ast::Const) {
        let ty = self.tcx.value_ty(def);
        if let Some(body) = konst.body() {
            self.check_expr(&body, &ty);
        }
    }

    /// Bring a local into scope.
    pub fn bind(&mut self, name: SmolStr, ty: Ty) {
        self.locals.push((name, ty));
    }

    /// The type of the innermost local named `name`.
    pub fn local(&self, name: &str) -> Option<Ty> {
        self.locals
            .iter()
            .rev()
            .find(|(local, _)| local == name)
            .map(|(_, ty)| ty.clone())
    }

    /// Check statements in the current scope (inline `<r>` content).
    pub fn check_stmts(&mut self, stmts: impl Iterator<Item = ast::Stmt>) {
        for stmt in stmts {
            self.check_stmt(&stmt);
        }
    }

    /// Unify `actual` with `expected`; on failure report `type/mismatch` at `range`
    /// (an unmapped body range when `mapped` is false, a source range otherwise).
    pub fn expect_ty(&mut self, actual: &Ty, expected: &Ty, range: TextRange, mapped: bool) {
        if !self.unify(actual, expected) {
            let message = format!(
                "expected `{}`, found `{}`",
                self.display(expected),
                self.display(actual)
            );
            if mapped {
                let diag = Diagnostic::new(
                    self.env.file.path(self.tcx.db).clone(),
                    range,
                    DiagnosticCode::TypeMismatch,
                    message,
                );
                self.diagnostics.push(diag);
            } else {
                self.error(range, DiagnosticCode::TypeMismatch, message);
            }
        }
    }

    /// Finish the body: literal ranges, uninferred literals, match exhaustiveness.
    pub fn finish(mut self) -> (Vec<Diagnostic>, Vec<RefEdge>, Vec<RefEdge>) {
        let literals = std::mem::take(&mut self.literals);
        for (ty, value, range) in literals {
            match self.shallow(&ty) {
                Ty::Int(int) => {
                    let (min, max) = int.range();
                    if value < min || value > max {
                        self.error(
                            range,
                            DiagnosticCode::TypeLiteralOutOfRange,
                            format!("literal `{value}` is out of range for `{}`", int.name()),
                        );
                    }
                }
                Ty::Var(_) => self.error(
                    range,
                    DiagnosticCode::TypeCannotInfer,
                    "cannot infer the type of this integer literal; add a type annotation"
                        .to_owned(),
                ),
                _ => {}
            }
        }
        let matches = std::mem::take(&mut self.matches);
        for pending in matches {
            let scrutinee = self.deep(&pending.scrutinee);
            if contains_unknown(&scrutinee) {
                continue;
            }
            let rows: Vec<Vec<exhaustive::Pat>> =
                pending.rows.into_iter().map(|pat| vec![pat]).collect();
            let types = ExTypes { tcx: self.tcx };
            if let Some(witness) =
                exhaustive::missing(&types, std::slice::from_ref(&scrutinee), &rows)
            {
                let shown = witness
                    .first()
                    .map(|w| self.format_witness(w, &scrutinee))
                    .unwrap_or_else(|| "_".to_owned());
                self.error(
                    pending.range,
                    DiagnosticCode::TypeNonExhaustiveMatch,
                    format!("non-exhaustive match: `{shown}` not covered"),
                );
            }
        }
        (self.diagnostics, self.calls, self.const_refs)
    }

    // ---- diagnostics ----------------------------------------------------------------------

    fn source_range(&self, range: TextRange) -> TextRange {
        match self.map {
            Some(map) => map.range_to_source(range),
            None => range,
        }
    }

    fn error(&mut self, range: TextRange, code: DiagnosticCode, message: String) {
        let range = self.source_range(range);
        let diag = Diagnostic::new(
            self.env.file.path(self.tcx.db).clone(),
            range,
            code,
            message,
        );
        self.diagnostics.push(diag);
    }

    fn span(&self, range: TextRange) -> Span {
        Span::new(self.env.file, self.source_range(range))
    }

    fn lower(&mut self, ty: &ast::Type) -> Ty {
        let mut out = Vec::new();
        let lowered = self.tcx.lower_type(&self.env, ty, &mut out);
        for mut diag in out {
            diag.range = self.source_range(diag.range);
            self.diagnostics.push(diag);
        }
        lowered
    }

    fn turbofish(&mut self, list: Option<ast::GenericArgList>) -> Option<Vec<Ty>> {
        list.map(|list| list.types().map(|ty| self.lower(&ty)).collect())
    }

    // ---- unification ----------------------------------------------------------------------

    fn fresh(&mut self, kind: VarKind) -> Ty {
        let index = u32::try_from(self.slots.len()).unwrap_or(u32::MAX);
        self.slots.push(Slot::Unbound(kind));
        Ty::Var(index)
    }

    fn fresh_n(&mut self, count: usize) -> Vec<Ty> {
        (0..count).map(|_| self.fresh(VarKind::General)).collect()
    }

    fn var_kind(&self, var: u32) -> Option<VarKind> {
        match self.slots.get(var as usize) {
            Some(Slot::Unbound(kind)) => Some(*kind),
            _ => None,
        }
    }

    /// Follow bound variables at the top level.
    fn shallow(&self, ty: &Ty) -> Ty {
        let mut current = ty.clone();
        let mut steps = 0usize;
        while let Ty::Var(var) = current {
            match self.slots.get(var as usize) {
                Some(Slot::Bound(next)) if steps < self.slots.len() => {
                    current = next.clone();
                    steps = steps.saturating_add(1);
                }
                _ => break,
            }
        }
        current
    }

    /// Resolve every bound variable.
    fn deep(&self, ty: &Ty) -> Ty {
        match self.shallow(ty) {
            Ty::Adt { def, args } => Ty::Adt {
                def,
                args: args.iter().map(|arg| self.deep(arg)).collect(),
            },
            other => other,
        }
    }

    fn occurs(&self, var: u32, ty: &Ty) -> bool {
        match self.shallow(ty) {
            Ty::Var(other) => other == var,
            Ty::Adt { args, .. } => args.iter().any(|arg| self.occurs(var, arg)),
            _ => false,
        }
    }

    fn unify(&mut self, a: &Ty, b: &Ty) -> bool {
        let a = self.shallow(a);
        let b = self.shallow(b);
        match (&a, &b) {
            (Ty::Error, Ty::Var(var)) | (Ty::Var(var), Ty::Error) => {
                // An unknown type absorbs the variable, so no follow-up errors appear.
                if let Some(slot) = self.slots.get_mut(*var as usize) {
                    *slot = Slot::Bound(Ty::Error);
                }
                true
            }
            (Ty::Error, _) | (_, Ty::Error) => true,
            (Ty::Var(x), Ty::Var(y)) if x == y => true,
            (Ty::Var(x), Ty::Var(y)) => {
                if self.var_kind(*x) == Some(VarKind::IntLiteral)
                    && self.var_kind(*y) == Some(VarKind::General)
                {
                    self.bind_var(*y, a.clone())
                } else {
                    self.bind_var(*x, b.clone())
                }
            }
            (Ty::Var(x), _) => self.bind_var(*x, b.clone()),
            (_, Ty::Var(y)) => self.bind_var(*y, a.clone()),
            (Ty::Adt { def: d1, args: a1 }, Ty::Adt { def: d2, args: a2 }) => {
                if d1 != d2 || a1.len() != a2.len() {
                    return false;
                }
                let mut ok = true;
                for (x, y) in a1.iter().zip(a2.iter()) {
                    ok &= self.unify(x, y);
                }
                ok
            }
            _ => a == b,
        }
    }

    fn bind_var(&mut self, var: u32, ty: Ty) -> bool {
        if self.var_kind(var) == Some(VarKind::IntLiteral)
            && !matches!(ty, Ty::Int(_))
            && !matches!(&ty, Ty::Var(other) if self.var_kind(*other).is_some())
        {
            return false;
        }
        if self.occurs(var, &ty) {
            return false;
        }
        if let Some(slot) = self.slots.get_mut(var as usize) {
            *slot = Slot::Bound(ty);
        }
        true
    }

    fn display(&self, ty: &Ty) -> String {
        let ty = self.deep(ty);
        self.tcx.display(&ty, &|var| match self.var_kind(var) {
            Some(VarKind::IntLiteral) => "{integer}".to_owned(),
            _ => "_".to_owned(),
        })
    }

    // ---- statements and expressions -------------------------------------------------------

    fn check_stmt(&mut self, stmt: &ast::Stmt) {
        match stmt {
            ast::Stmt::LetStmt(stmt) => {
                let expected = match stmt.ty() {
                    Some(ty) => self.lower(&ty),
                    None => self.fresh(VarKind::General),
                };
                if let Some(init) = stmt.initializer() {
                    self.check_expr(&init, &expected);
                }
                match stmt.pat() {
                    Some(ast::Pat::IdentPat(pat)) => {
                        if let Some(name) = pat.name() {
                            self.bind(SmolStr::new(name.text()), expected);
                        }
                    }
                    Some(ast::Pat::WildcardPat(_)) | None => {}
                    Some(other) => self.error(
                        other.syntax().text_range(),
                        DiagnosticCode::TypeBadPattern,
                        "`let` only binds a name or `_`".to_owned(),
                    ),
                }
            }
            ast::Stmt::ExprStmt(stmt) => {
                let Some(expr) = stmt.expr() else {
                    return;
                };
                let expected = self.fresh(VarKind::General);
                let ty = self.check_expr(&expr, &expected);
                if let Ty::Adt { def, .. } = self.shallow(&ty)
                    && self.tcx.has_attr(def, "must_use")
                {
                    let name = self.tcx.name(def);
                    self.error(
                        expr.syntax().text_range(),
                        DiagnosticCode::TypeUnappliedChange,
                        format!("unused `{name}`: the change has no effect until it is applied"),
                    );
                }
            }
        }
    }

    /// Infer `expr` with `expected` as a hint, then unify the two.
    fn check_expr(&mut self, expr: &ast::Expr, expected: &Ty) -> Ty {
        let ty = self.infer_expr(expr, expected);
        self.expect_ty(&ty, expected, expr.syntax().text_range(), false);
        ty
    }

    fn check_opt(&mut self, expr: Option<ast::Expr>, expected: &Ty) -> Ty {
        match expr {
            Some(expr) => self.check_expr(&expr, expected),
            None => Ty::Error,
        }
    }

    fn infer_expr(&mut self, expr: &ast::Expr, expected: &Ty) -> Ty {
        match expr {
            ast::Expr::Literal(literal) => self.literal(literal, false),
            ast::Expr::PathExpr(path_expr) => match path_expr.path() {
                Some(path) => self.path_value(&path),
                None => Ty::Error,
            },
            ast::Expr::RecordExpr(record) => self.record(record),
            ast::Expr::ParenExpr(paren) => match paren.expr() {
                Some(inner) => self.infer_expr(&inner, expected),
                None => Ty::Error,
            },
            ast::Expr::UnitExpr(_) => Ty::Unit,
            ast::Expr::BlockExpr(block) => self.block(block, expected),
            ast::Expr::MatchExpr(matched) => self.match_expr(matched, expected),
            ast::Expr::CallExpr(call) => self.call(call),
            ast::Expr::MethodCallExpr(call) => self.method_call(call),
            ast::Expr::FieldExpr(field) => self.field(field),
            ast::Expr::CastExpr(cast) => self.cast(cast),
            ast::Expr::BinExpr(bin) => self.binary(bin),
            ast::Expr::PrefixExpr(prefix) => self.prefix(prefix),
        }
    }

    fn literal(&mut self, literal: &ast::Literal, negative: bool) -> Ty {
        let range = literal.syntax().text_range();
        match literal.kind() {
            Some(LiteralKind::Int) => {
                let var = self.fresh(VarKind::IntLiteral);
                match literal
                    .int_value()
                    .and_then(|value| i128::try_from(value).ok())
                {
                    Some(value) => {
                        let value = if negative {
                            value.saturating_neg()
                        } else {
                            value
                        };
                        self.literals.push((var.clone(), value, range));
                    }
                    None => self.error(
                        range,
                        DiagnosticCode::TypeLiteralOutOfRange,
                        "integer literal is too large".to_owned(),
                    ),
                }
                var
            }
            Some(LiteralKind::Decimal) => Ty::Rational,
            Some(LiteralKind::Str) => Ty::Str,
            Some(LiteralKind::Bool(_)) => Ty::Bool,
            None => Ty::Error,
        }
    }

    fn block(&mut self, block: &ast::BlockExpr, expected: &Ty) -> Ty {
        let mark = self.locals.len();
        for stmt in block.stmts() {
            self.check_stmt(&stmt);
        }
        match block.tail_expr() {
            Some(tail) => {
                self.check_expr(&tail, expected);
            }
            None => {
                let range = block
                    .syntax()
                    .last_token()
                    .map(|token| token.text_range())
                    .unwrap_or_else(|| block.syntax().text_range());
                self.expect_ty(&Ty::Unit, expected, range, false);
            }
        }
        self.locals.truncate(mark);
        expected.clone()
    }

    fn match_expr(&mut self, matched: &ast::MatchExpr, expected: &Ty) -> Ty {
        let scrutinee_ty = self.fresh(VarKind::General);
        let scrutinee = matched.scrutinee();
        let range = scrutinee
            .as_ref()
            .map(|expr| expr.syntax().text_range())
            .unwrap_or_else(|| matched.syntax().text_range());
        let scrutinee_ty = self.check_opt(scrutinee, &scrutinee_ty);
        let errors_before = self.pat_error;
        self.pat_error = false;
        let mut rows = Vec::new();
        for arm in matched.arms() {
            let mark = self.locals.len();
            if let Some(pat) = arm.pat() {
                let mut binds = Vec::new();
                rows.push(self.check_pat(&pat, &scrutinee_ty, &mut binds));
                for (name, ty) in binds {
                    self.bind(name, ty);
                }
            } else {
                self.pat_error = true;
            }
            self.check_opt(arm.expr(), expected);
            self.locals.truncate(mark);
        }
        if !self.pat_error {
            self.matches.push(PendingMatch {
                scrutinee: scrutinee_ty,
                rows,
                range,
            });
        }
        self.pat_error = errors_before;
        expected.clone()
    }

    fn binary(&mut self, bin: &ast::BinExpr) -> Ty {
        let Some(op) = bin.op_kind() else {
            return Ty::Error;
        };
        let range = bin.syntax().text_range();
        if op.is_logical() {
            self.check_opt(bin.lhs(), &Ty::Bool);
            self.check_opt(bin.rhs(), &Ty::Bool);
            return Ty::Bool;
        }
        let operand = self.fresh(VarKind::General);
        self.check_opt(bin.lhs(), &operand);
        self.check_opt(bin.rhs(), &operand);
        let resolved = self.shallow(&operand);
        let unknown = matches!(resolved, Ty::Error | Ty::Var(_));
        let valid = unknown
            || match op {
                BinOp::Eq | BinOp::Ne => true,
                BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                    matches!(resolved, Ty::Int(_) | Ty::Rational)
                }
                BinOp::Add => matches!(resolved, Ty::Int(_) | Ty::Rational | Ty::Str),
                BinOp::Sub | BinOp::Mul | BinOp::Div => {
                    matches!(resolved, Ty::Int(_) | Ty::Rational)
                }
                BinOp::Rem => matches!(resolved, Ty::Int(_)),
                BinOp::And | BinOp::Or => false,
            };
        if !valid {
            let shown = self.display(&resolved);
            self.error(
                range,
                DiagnosticCode::TypeInvalidOperator,
                format!("operator `{}` cannot be applied to `{shown}`", op.as_str()),
            );
        }
        if op.is_arithmetic() {
            if valid { operand } else { Ty::Error }
        } else {
            Ty::Bool
        }
    }

    fn prefix(&mut self, prefix: &ast::PrefixExpr) -> Ty {
        let range = prefix.syntax().text_range();
        match prefix.op_kind() {
            Some(PrefixOp::Not) => {
                self.check_opt(prefix.expr(), &Ty::Bool);
                Ty::Bool
            }
            Some(PrefixOp::Neg) => {
                if let Some(ast::Expr::Literal(literal)) = prefix.expr()
                    && literal.kind() == Some(LiteralKind::Int)
                {
                    return self.literal(&literal, true);
                }
                let operand = self.fresh(VarKind::General);
                self.check_opt(prefix.expr(), &operand);
                let resolved = self.shallow(&operand);
                let valid = match &resolved {
                    Ty::Int(int) => int.is_signed(),
                    Ty::Rational | Ty::Error | Ty::Var(_) => true,
                    _ => false,
                };
                if valid {
                    operand
                } else {
                    let shown = self.display(&resolved);
                    self.error(
                        range,
                        DiagnosticCode::TypeInvalidOperator,
                        format!("unary `-` cannot be applied to `{shown}`"),
                    );
                    Ty::Error
                }
            }
            None => Ty::Error,
        }
    }

    fn cast(&mut self, cast: &ast::CastExpr) -> Ty {
        let inner = self.fresh(VarKind::General);
        let inner = self.check_opt(cast.expr(), &inner);
        let target = match cast.ty() {
            Some(ty) => self.lower(&ty),
            None => Ty::Error,
        };
        let numeric = |body: &Self, ty: &Ty| match body.shallow(ty) {
            Ty::Int(_) | Ty::Rational | Ty::Error => true,
            Ty::Var(var) => body.var_kind(var) == Some(VarKind::IntLiteral),
            _ => false,
        };
        if !numeric(self, &inner) || !numeric(self, &target) {
            let message = format!(
                "cannot cast `{}` as `{}`: only numeric casts are allowed",
                self.display(&inner),
                self.display(&target)
            );
            self.error(
                cast.syntax().text_range(),
                DiagnosticCode::TypeInvalidCast,
                message,
            );
        }
        target
    }

    fn field(&mut self, field: &ast::FieldExpr) -> Ty {
        let receiver = self.fresh(VarKind::General);
        let receiver = self.check_opt(field.receiver(), &receiver);
        let Some(name_ref) = field.name_ref() else {
            return Ty::Error;
        };
        let name = name_ref.text();
        let range = name_ref.syntax().text_range();
        match self.shallow(&receiver) {
            Ty::Error => Ty::Error,
            Ty::Var(_) => {
                self.error(
                    range,
                    DiagnosticCode::TypeCannotInfer,
                    format!("cannot infer the type whose field `{name}` is accessed"),
                );
                Ty::Error
            }
            Ty::Adt { def, args } => {
                let found = self
                    .tcx
                    .struct_fields(def)
                    .and_then(|fields| fields.iter().find(|(field, _)| *field == name).cloned());
                match found {
                    Some((_, ty)) => subst(&ty, &[(def, args)]),
                    None => self.no_field(range, &name, &Ty::Adt { def, args }),
                }
            }
            other => self.no_field(range, &name, &other),
        }
    }

    fn no_field(&mut self, range: TextRange, name: &str, ty: &Ty) -> Ty {
        let shown = self.display(ty);
        self.error(
            range,
            DiagnosticCode::TypeNoField,
            format!("no field `{name}` on type `{shown}`"),
        );
        Ty::Error
    }

    fn record(&mut self, record: &ast::RecordExpr) -> Ty {
        let Some(path) = record.path() else {
            return Ty::Error;
        };
        let Some((ty, def, args)) = self.struct_path(&path) else {
            for field in record.fields() {
                if let Some(expr) = field.expr() {
                    self.check_expr(&expr, &Ty::Error);
                }
            }
            if let Some(spread) = record.spread() {
                self.check_expr(&spread, &Ty::Error);
            }
            return Ty::Error;
        };
        let fields = self.tcx.struct_fields(def).unwrap_or_default();
        let mut seen: Vec<SmolStr> = Vec::new();
        for field in record.fields() {
            let Some(name_ref) = field.name_ref() else {
                continue;
            };
            let name = SmolStr::new(name_ref.text());
            let range = name_ref.syntax().text_range();
            let field_ty = fields
                .iter()
                .find(|(field, _)| *field == name)
                .map(|(_, field_ty)| subst(field_ty, &[(def, args.clone())]));
            let expected = match field_ty {
                Some(field_ty) if !seen.contains(&name) => {
                    seen.push(name.clone());
                    field_ty
                }
                Some(_) => {
                    self.error(
                        range,
                        DiagnosticCode::TypeNoField,
                        format!("field `{name}` is specified more than once"),
                    );
                    Ty::Error
                }
                None => {
                    self.no_field(range, &name, &ty);
                    Ty::Error
                }
            };
            match field.expr() {
                Some(expr) => {
                    self.check_expr(&expr, &expected);
                }
                None => {
                    let value = self.name_value(&name, range);
                    self.expect_ty(&value, &expected, range, false);
                }
            }
        }
        match record.spread() {
            Some(spread) => {
                self.check_expr(&spread, &ty);
            }
            None => {
                let missing: Vec<String> = fields
                    .iter()
                    .filter(|(field, _)| !seen.contains(field))
                    .map(|(field, _)| format!("`{field}`"))
                    .collect();
                if !missing.is_empty() {
                    self.error(
                        path.syntax().text_range(),
                        DiagnosticCode::TypeMissingFields,
                        format!("missing field(s) {}", missing.join(", ")),
                    );
                }
            }
        }
        ty
    }

    /// Resolve the path of a record expression or pattern to a struct with named fields.
    fn struct_path(&mut self, path: &ast::Path) -> Option<(Ty, DefId, Vec<Ty>)> {
        let segments: Vec<ast::PathSegment> = path.segments().collect();
        let last = segments.last()?;
        let mut out = Vec::new();
        let resolved = self.tcx.resolve_type_path(&self.env, &segments, &mut out);
        for mut diag in out {
            diag.range = self.source_range(diag.range);
            self.diagnostics.push(diag);
        }
        let ty = match resolved? {
            TypeRes::Ty(ty) => ty,
            TypeRes::Def(def) => {
                let count = self.tcx.item(def)?.kind.generics().len();
                let args = match self.turbofish(last.generic_arg_list()) {
                    Some(args) => args,
                    None => self.fresh_n(count),
                };
                let mut out = Vec::new();
                let ty = self.tcx.instantiate(
                    &self.env,
                    path.syntax().text_range(),
                    def,
                    args,
                    &mut out,
                );
                for mut diag in out {
                    diag.range = self.source_range(diag.range);
                    self.diagnostics.push(diag);
                }
                ty
            }
        };
        match &ty {
            Ty::Adt { def, args } if self.tcx.struct_fields(*def).is_some() => {
                Some((ty.clone(), *def, args.clone()))
            }
            Ty::Error => None,
            _ => {
                let shown = self.display(&ty);
                self.error(
                    path.syntax().text_range(),
                    DiagnosticCode::TypeUnresolvedType,
                    format!("`{shown}` is not a struct with named fields"),
                );
                None
            }
        }
    }

    // ---- paths and calls ------------------------------------------------------------------

    /// The value of a single name (a record shorthand).
    fn name_value(&mut self, name: &str, range: TextRange) -> Ty {
        if let Some(ty) = self.local(name) {
            return ty;
        }
        match self.lookup_global(name, false) {
            Some(res) => self.res_value(res, name, range),
            None => {
                self.error(
                    range,
                    DiagnosticCode::TypeUnresolvedName,
                    format!("cannot find value `{name}`"),
                );
                Ty::Error
            }
        }
    }

    /// A bare name outside the locals: the `.kirp` fns, then the root value namespace.
    fn lookup_global(&mut self, name: &str, pattern: bool) -> Option<ValueRes> {
        if !pattern && let Some(&def) = self.kirp_fns.get(name) {
            return Some(ValueRes::Def {
                def,
                impl_args: None,
                fn_args: None,
            });
        }
        match *self.tcx.scope.root.values.get(name)? {
            ValueDef::Item(def) => Some(ValueRes::Def {
                def,
                impl_args: None,
                fn_args: None,
            }),
            ValueDef::Variant { enum_def, index } => {
                let count = self.tcx.item(enum_def)?.kind.generics().len();
                let args = self.fresh_n(count);
                Some(ValueRes::Variant {
                    enum_def,
                    index,
                    args,
                })
            }
        }
    }

    /// Resolve a value path; `None` after reporting an error.
    fn resolve_value(&mut self, path: &ast::Path, pattern: bool) -> Option<ValueRes> {
        let segments: Vec<ast::PathSegment> = path.segments().collect();
        let (last, prefix) = segments.split_last()?;
        if prefix.is_empty() {
            return self.resolve_single(last, pattern);
        }
        // Resolve the prefix to a module or a type.
        let mut module: Option<DefId> = None;
        let mut through: Option<Ty> = None;
        for (position, segment) in prefix.iter().enumerate() {
            let range = segment.syntax().text_range();
            if through.is_some() {
                self.error(
                    range,
                    DiagnosticCode::TypeUnresolvedName,
                    "associated items cannot be nested".to_owned(),
                );
                return None;
            }
            match segment.kind()? {
                ast::PathSegmentKind::SelfType if position == 0 => match self.env.self_ty.clone() {
                    Some(ty) => through = Some(ty),
                    None => {
                        self.error(
                            range,
                            DiagnosticCode::TypeSelfOutsideImpl,
                            "`Self` is only available inside an impl".to_owned(),
                        );
                        return None;
                    }
                },
                ast::PathSegmentKind::Name(name) => {
                    let text = name.text();
                    let found = self
                        .tcx
                        .scope
                        .module(module)
                        .and_then(|scope| scope.types.get(text.as_str()))
                        .copied();
                    let Some(def) = found else {
                        self.error(
                            range,
                            DiagnosticCode::TypeUnresolvedName,
                            format!("cannot find `{text}`"),
                        );
                        return None;
                    };
                    let kind = &self.tcx.item(def)?.kind;
                    if matches!(kind, ItemKind::Mod { .. }) {
                        module = Some(def);
                        continue;
                    }
                    let count = kind.generics().len();
                    let args = match self.turbofish(segment.generic_arg_list()) {
                        Some(args) => args,
                        None => self.fresh_n(count),
                    };
                    let mut out = Vec::new();
                    let ty = self.tcx.instantiate(&self.env, range, def, args, &mut out);
                    for mut diag in out {
                        diag.range = self.source_range(diag.range);
                        self.diagnostics.push(diag);
                    }
                    through = Some(ty);
                }
                _ => {
                    self.error(
                        range,
                        DiagnosticCode::TypeUnresolvedName,
                        "unexpected `self` in a path".to_owned(),
                    );
                    return None;
                }
            }
        }
        let range = last.syntax().text_range();
        let ast::PathSegmentKind::Name(name) = last.kind()? else {
            self.error(
                range,
                DiagnosticCode::TypeUnresolvedName,
                "unexpected `self` in a path".to_owned(),
            );
            return None;
        };
        let text = name.text();
        let fn_args = self.turbofish(last.generic_arg_list());
        match through {
            None => {
                let found = self
                    .tcx
                    .scope
                    .module(module)
                    .and_then(|scope| scope.values.get(text.as_str()))
                    .copied();
                match found {
                    Some(ValueDef::Item(def)) => Some(ValueRes::Def {
                        def,
                        impl_args: None,
                        fn_args,
                    }),
                    Some(ValueDef::Variant { enum_def, index }) => {
                        let count = self.tcx.item(enum_def)?.kind.generics().len();
                        let args = self.fresh_n(count);
                        Some(ValueRes::Variant {
                            enum_def,
                            index,
                            args,
                        })
                    }
                    None => {
                        let module_name = module.map(|def| self.tcx.name(def)).unwrap_or_default();
                        self.error(
                            range,
                            DiagnosticCode::TypeUnresolvedName,
                            format!("cannot find `{text}` in `{module_name}`"),
                        );
                        None
                    }
                }
            }
            Some(Ty::Error) => None,
            Some(ty) => {
                if let Ty::Adt { def, args } = self.shallow(&ty) {
                    if let Some(ItemKind::Enum { variants, .. }) =
                        self.tcx.item(def).map(|item| &item.kind)
                        && let Some(index) = variants.iter().position(|(v, _, _)| *v == text)
                    {
                        return Some(ValueRes::Variant {
                            enum_def: def,
                            index: u32::try_from(index).ok()?,
                            args,
                        });
                    }
                    if let Some(&method) = self
                        .tcx
                        .scope
                        .methods
                        .get(&def)
                        .and_then(|methods| methods.get(text.as_str()))
                    {
                        return Some(ValueRes::Def {
                            def: method,
                            impl_args: Some(args),
                            fn_args,
                        });
                    }
                }
                let shown = self.display(&ty);
                self.error(
                    range,
                    DiagnosticCode::TypeUnresolvedName,
                    format!("no associated item `{text}` on `{shown}`"),
                );
                None
            }
        }
    }

    fn resolve_single(&mut self, segment: &ast::PathSegment, pattern: bool) -> Option<ValueRes> {
        let range = segment.syntax().text_range();
        match segment.kind()? {
            ast::PathSegmentKind::SelfValue => {
                if self.self_local && !pattern {
                    return self.local("self").map(ValueRes::Local);
                }
                let (code, message) = if self.env.self_ty.is_none() {
                    (
                        DiagnosticCode::TypeSelfOutsideImpl,
                        "`self` is only available inside an impl",
                    )
                } else {
                    (
                        DiagnosticCode::TypeUnresolvedName,
                        "`self` is only available in a function with a `self` parameter",
                    )
                };
                self.error(range, code, message.to_owned());
                None
            }
            ast::PathSegmentKind::SelfType => match self.env.self_ty.clone() {
                Some(Ty::Adt { def, args }) if self.tcx.struct_fields(def).is_none() => {
                    Some(ValueRes::Def {
                        def,
                        impl_args: Some(args),
                        fn_args: None,
                    })
                }
                Some(_) => {
                    self.error(
                        range,
                        DiagnosticCode::TypeUnresolvedName,
                        "`Self` is not a unit struct".to_owned(),
                    );
                    None
                }
                None => {
                    self.error(
                        range,
                        DiagnosticCode::TypeSelfOutsideImpl,
                        "`Self` is only available inside an impl".to_owned(),
                    );
                    None
                }
            },
            ast::PathSegmentKind::Name(name) => {
                let text = name.text();
                if !pattern && let Some(ty) = self.local(&text) {
                    return Some(ValueRes::Local(ty));
                }
                let fn_args = self.turbofish(segment.generic_arg_list());
                match self.lookup_global(&text, pattern) {
                    Some(ValueRes::Def { def, impl_args, .. }) => Some(ValueRes::Def {
                        def,
                        impl_args,
                        fn_args,
                    }),
                    Some(ValueRes::Variant {
                        enum_def,
                        index,
                        args,
                    }) => {
                        let args = fn_args.unwrap_or(args);
                        Some(ValueRes::Variant {
                            enum_def,
                            index,
                            args,
                        })
                    }
                    Some(local) => Some(local),
                    None => {
                        self.error(
                            range,
                            DiagnosticCode::TypeUnresolvedName,
                            format!("cannot find value `{text}`"),
                        );
                        None
                    }
                }
            }
        }
    }

    fn path_value(&mut self, path: &ast::Path) -> Ty {
        let name = path.syntax().text().to_string();
        let range = path.syntax().text_range();
        match self.resolve_value(path, false) {
            Some(res) => self.res_value(res, &name, range),
            None => Ty::Error,
        }
    }

    /// The type of a resolved path used as a value (not called).
    fn res_value(&mut self, res: ValueRes, name: &str, range: TextRange) -> Ty {
        match res {
            ValueRes::Local(ty) => ty,
            ValueRes::Variant {
                enum_def,
                index,
                args,
            } => {
                let arity = self
                    .tcx
                    .enum_variants(enum_def)
                    .get(index as usize)
                    .map(|(_, payload)| payload.len())
                    .unwrap_or(0);
                if arity == 0 {
                    Ty::Adt {
                        def: enum_def,
                        args,
                    }
                } else {
                    self.not_callable_value(range, name)
                }
            }
            ValueRes::Def {
                def,
                impl_args,
                fn_args: _,
            } => {
                let Some(item) = self.tcx.item(def) else {
                    return Ty::Error;
                };
                match &item.kind {
                    ItemKind::Const => {
                        self.record_ref(def, range, true);
                        let ty = self.tcx.value_ty(def);
                        match (self.tcx.parent_impl(def), impl_args) {
                            (Some(imp), Some(args)) => subst(&ty, &[(imp, args)]),
                            _ => ty,
                        }
                    }
                    ItemKind::AssetImport { .. } => self.tcx.value_ty(def),
                    ItemKind::Struct { generics, .. } => {
                        let args = impl_args.unwrap_or_else(|| self.fresh_n(generics.len()));
                        Ty::Adt { def, args }
                    }
                    ItemKind::Fn { .. } => self.not_callable_value(range, name),
                    _ => Ty::Error,
                }
            }
        }
    }

    fn not_callable_value(&mut self, range: TextRange, name: &str) -> Ty {
        self.error(
            range,
            DiagnosticCode::TypeNotCallable,
            format!("`{name}` must be called with arguments"),
        );
        Ty::Error
    }

    fn record_ref(&mut self, to: DefId, range: TextRange, is_const: bool) {
        let Some(from) = self.owner else {
            return;
        };
        let edge = RefEdge {
            from,
            to,
            span: self.span(range),
        };
        if is_const {
            self.const_refs.push(edge);
        } else {
            self.calls.push(edge);
        }
    }

    fn args(list: Option<ast::ArgList>) -> (Vec<ast::Expr>, Option<TextRange>) {
        match list {
            Some(list) => (list.args().collect(), Some(list.syntax().text_range())),
            None => (Vec::new(), None),
        }
    }

    /// Check call arguments against parameter types.
    fn check_args(&mut self, args: &[ast::Expr], params: &[Ty], range: TextRange) {
        if args.len() != params.len() {
            self.error(
                range,
                DiagnosticCode::TypeWrongArgCount,
                format!(
                    "expected {} argument(s), found {}",
                    params.len(),
                    args.len()
                ),
            );
        }
        for (position, arg) in args.iter().enumerate() {
            match params.get(position) {
                Some(param) => {
                    self.check_expr(arg, param);
                }
                None => {
                    self.check_expr(arg, &Ty::Error);
                }
            }
        }
    }

    /// Instantiate the generics of the fn `def`: the impl's from `impl_args` (or fresh), the
    /// fn's own from the turbofish (or fresh).
    fn fn_subst(
        &mut self,
        def: DefId,
        impl_args: Option<Vec<Ty>>,
        fn_args: Option<Vec<Ty>>,
        range: TextRange,
    ) -> Vec<(DefId, Vec<Ty>)> {
        let sig = self.tcx.fn_sig(def);
        let mut map = Vec::new();
        if let Some(imp) = sig.impl_def {
            let count = self
                .tcx
                .item(imp)
                .map(|item| item.kind.generics().len())
                .unwrap_or(0);
            let args = match impl_args {
                Some(args) if args.len() == count => args,
                _ => self.fresh_n(count),
            };
            map.push((imp, args));
        }
        let count = self
            .tcx
            .item(def)
            .map(|item| item.kind.generics().len())
            .unwrap_or(0);
        let args = match fn_args {
            Some(args) if args.len() == count => args,
            Some(args) => {
                self.error(
                    range,
                    DiagnosticCode::TypeWrongGenericCount,
                    format!(
                        "`{}` takes {count} generic argument(s), but {} were given",
                        self.tcx.name(def),
                        args.len()
                    ),
                );
                self.fresh_n(count)
            }
            None => self.fresh_n(count),
        };
        map.push((def, args));
        map
    }

    fn call(&mut self, call: &ast::CallExpr) -> Ty {
        let (args, list_range) = Self::args(call.arg_list());
        let range = list_range.unwrap_or_else(|| call.syntax().text_range());
        let callee = call.callee();
        let resolved = match &callee {
            Some(ast::Expr::PathExpr(path_expr)) => path_expr
                .path()
                .map(|path| (self.resolve_value(&path, false), path)),
            _ => None,
        };
        let Some((res, path)) = resolved else {
            // Not a path: whatever it is, it cannot be called.
            if let Some(callee) = &callee {
                let any = self.fresh(VarKind::General);
                let ty = self.check_expr(callee, &any);
                if !matches!(self.shallow(&ty), Ty::Error) {
                    self.error(
                        callee.syntax().text_range(),
                        DiagnosticCode::TypeNotCallable,
                        "this expression is not a function".to_owned(),
                    );
                }
            }
            self.check_args(&args, &[], range);
            return Ty::Error;
        };
        let path_range = path.syntax().text_range();
        let name = path.syntax().text().to_string();
        let Some(res) = res else {
            let params: Vec<Ty> = args.iter().map(|_| Ty::Error).collect();
            self.check_args(&args, &params, range);
            return Ty::Error;
        };
        match res {
            ValueRes::Variant {
                enum_def,
                index,
                args: enum_args,
            } => {
                let payload = self
                    .tcx
                    .enum_variants(enum_def)
                    .get(index as usize)
                    .map(|(_, payload)| payload.clone())
                    .unwrap_or_default();
                if payload.is_empty() {
                    self.error(
                        path_range,
                        DiagnosticCode::TypeNotCallable,
                        format!("`{name}` is a unit variant, not a function"),
                    );
                    self.check_args(&args, &[], range);
                    return Ty::Error;
                }
                let map = [(enum_def, enum_args.clone())];
                let params: Vec<Ty> = payload.iter().map(|ty| subst(ty, &map)).collect();
                self.check_args(&args, &params, range);
                Ty::Adt {
                    def: enum_def,
                    args: enum_args,
                }
            }
            ValueRes::Def {
                def,
                impl_args,
                fn_args,
            } if matches!(
                self.tcx.item(def).map(|item| &item.kind),
                Some(ItemKind::Fn { .. })
            ) =>
            {
                let map = self.fn_subst(def, impl_args, fn_args, path_range);
                let sig = self.tcx.fn_sig(def);
                let mut params = Vec::new();
                if sig.has_self {
                    params.push(subst(sig.self_ty.as_ref().unwrap_or(&Ty::Error), &map));
                }
                params.extend(sig.params.iter().map(|ty| subst(ty, &map)));
                self.check_args(&args, &params, range);
                if !sig.builtin {
                    self.record_ref(def, path_range, false);
                }
                subst(&sig.ret, &map)
            }
            other => {
                let ty = self.res_value(other, &name, path_range);
                if !matches!(self.shallow(&ty), Ty::Error) {
                    self.error(
                        path_range,
                        DiagnosticCode::TypeNotCallable,
                        format!("`{name}` is not a function"),
                    );
                }
                self.check_args(&args, &[], range);
                Ty::Error
            }
        }
    }

    fn method_call(&mut self, call: &ast::MethodCallExpr) -> Ty {
        let receiver = self.fresh(VarKind::General);
        let receiver = self.check_opt(call.receiver(), &receiver);
        let (args, list_range) = Self::args(call.arg_list());
        let range = list_range.unwrap_or_else(|| call.syntax().text_range());
        let Some(name_ref) = call.name_ref() else {
            return Ty::Error;
        };
        let name = name_ref.text();
        let name_range = name_ref.syntax().text_range();
        let fn_args = self.turbofish(call.generic_arg_list());
        let unknown: Vec<Ty> = args.iter().map(|_| Ty::Error).collect();
        let (def, adt_args) = match self.shallow(&receiver) {
            Ty::Error => {
                self.check_args(&args, &unknown, range);
                return Ty::Error;
            }
            Ty::Var(_) => {
                self.error(
                    name_range,
                    DiagnosticCode::TypeCannotInfer,
                    format!("cannot infer the type on which `{name}` is called"),
                );
                self.check_args(&args, &unknown, range);
                return Ty::Error;
            }
            Ty::Adt { def, args } => (def, args),
            other => {
                self.no_method(name_range, &name, &other);
                self.check_args(&args, &unknown, range);
                return Ty::Error;
            }
        };
        let method = self
            .tcx
            .scope
            .methods
            .get(&def)
            .and_then(|methods| methods.get(name.as_str()))
            .copied()
            .filter(|method| {
                matches!(
                    self.tcx.item(*method).map(|item| &item.kind),
                    Some(ItemKind::Fn { has_self: true, .. })
                )
            });
        let Some(method) = method else {
            let ty = Ty::Adt {
                def,
                args: adt_args,
            };
            self.no_method(name_range, &name, &ty);
            self.check_args(&args, &unknown, range);
            return Ty::Error;
        };
        let map = self.fn_subst(method, Some(adt_args), fn_args, name_range);
        let sig = self.tcx.fn_sig(method);
        let params: Vec<Ty> = sig.params.iter().map(|ty| subst(ty, &map)).collect();
        self.check_args(&args, &params, range);
        if !sig.builtin {
            self.record_ref(method, name_range, false);
        }
        subst(&sig.ret, &map)
    }

    fn no_method(&mut self, range: TextRange, name: &str, ty: &Ty) {
        let shown = self.display(ty);
        self.error(
            range,
            DiagnosticCode::TypeNoMethod,
            format!("no method `{name}` taking `self` on type `{shown}`"),
        );
    }

    // ---- patterns -------------------------------------------------------------------------

    fn bad_pattern(&mut self, range: TextRange, message: &str) -> exhaustive::Pat {
        self.pat_error = true;
        self.error(range, DiagnosticCode::TypeBadPattern, message.to_owned());
        exhaustive::Pat::Wild
    }

    fn expect_pat_ty(&mut self, actual: &Ty, expected: &Ty, range: TextRange) {
        if !self.unify(actual, expected) {
            self.pat_error = true;
            let message = format!(
                "this pattern has type `{}`, but the matched value has type `{}`",
                self.display(actual),
                self.display(expected)
            );
            self.error(range, DiagnosticCode::TypeMismatch, message);
        }
    }

    fn check_pat(
        &mut self,
        pat: &ast::Pat,
        ty: &Ty,
        binds: &mut Vec<(SmolStr, Ty)>,
    ) -> exhaustive::Pat {
        let range = pat.syntax().text_range();
        match pat {
            ast::Pat::WildcardPat(_) => exhaustive::Pat::Wild,
            ast::Pat::IdentPat(ident) => {
                let Some(name) = ident.name() else {
                    return exhaustive::Pat::Wild;
                };
                let text = name.text();
                match self.lookup_global(&text, true) {
                    Some(ValueRes::Variant {
                        enum_def,
                        index,
                        args,
                    }) => self.unit_variant_pat(enum_def, index, args, ty, range),
                    Some(ValueRes::Def { def, .. }) => match self.tcx.item(def).map(|i| &i.kind) {
                        Some(ItemKind::Const) => {
                            self.bad_pattern(range, "constants cannot be used as patterns")
                        }
                        Some(ItemKind::Struct {
                            fields: None,
                            generics,
                        }) => {
                            let args = self.fresh_n(generics.len());
                            self.expect_pat_ty(&Ty::Adt { def, args }, ty, range);
                            exhaustive::Pat::Struct(Vec::new())
                        }
                        _ => self.binding(SmolStr::new(text), ty, range, binds),
                    },
                    _ => self.binding(SmolStr::new(text), ty, range, binds),
                }
            }
            ast::Pat::LiteralPat(literal) => match self.literal_pat(literal, ty) {
                Some(LitPat::Int(value)) => exhaustive::Pat::Int(value, value),
                Some(LitPat::Bool(value)) => exhaustive::Pat::Bool(value),
                None => exhaustive::Pat::Wild,
            },
            ast::Pat::RangePat(range_pat) => {
                let is_inclusive = range_pat
                    .op_token()
                    .is_some_and(|op| op.kind() == kagari_syntax::represent::RepresentKind::DOT2EQ);
                if !is_inclusive {
                    return self.bad_pattern(range, "range patterns must use `..=`");
                }
                let lo = range_pat.start().and_then(|lo| self.literal_pat(&lo, ty));
                let hi = range_pat.end().and_then(|hi| self.literal_pat(&hi, ty));
                match (lo, hi) {
                    (Some(LitPat::Int(lo)), Some(LitPat::Int(hi))) if lo <= hi => {
                        exhaustive::Pat::Int(lo, hi)
                    }
                    (Some(LitPat::Int(_)), Some(LitPat::Int(_))) => {
                        self.bad_pattern(range, "the start of a range must not exceed its end")
                    }
                    (None, _) | (_, None) => {
                        self.pat_error = true;
                        exhaustive::Pat::Wild
                    }
                    _ => self.bad_pattern(range, "range bounds must be integers"),
                }
            }
            ast::Pat::TupleStructPat(tuple) => {
                let fields: Vec<ast::Pat> = tuple.fields().collect();
                let Some(path) = tuple.path() else {
                    self.pat_error = true;
                    return exhaustive::Pat::Wild;
                };
                match self.resolve_value(&path, true) {
                    Some(ValueRes::Variant {
                        enum_def,
                        index,
                        args,
                    }) => {
                        let payload = self
                            .tcx
                            .enum_variants(enum_def)
                            .get(index as usize)
                            .map(|(_, payload)| payload.clone())
                            .unwrap_or_default();
                        if payload.is_empty() || payload.len() != fields.len() {
                            return self.bad_pattern(
                                range,
                                &format!(
                                    "this variant has {} field(s), but the pattern has {}",
                                    payload.len(),
                                    fields.len()
                                ),
                            );
                        }
                        let adt = Ty::Adt {
                            def: enum_def,
                            args: args.clone(),
                        };
                        self.expect_pat_ty(&adt, ty, range);
                        let map = [(enum_def, args)];
                        let subs = fields
                            .iter()
                            .zip(payload.iter())
                            .map(|(field, field_ty)| {
                                self.check_pat(field, &subst(field_ty, &map), binds)
                            })
                            .collect();
                        exhaustive::Pat::Variant(index as usize, subs)
                    }
                    Some(_) => self.bad_pattern(range, "expected a tuple variant"),
                    None => {
                        self.pat_error = true;
                        exhaustive::Pat::Wild
                    }
                }
            }
            ast::Pat::RecordPat(record) => {
                let Some(path) = record.path() else {
                    self.pat_error = true;
                    return exhaustive::Pat::Wild;
                };
                let Some((adt, def, args)) = self.struct_path(&path) else {
                    self.pat_error = true;
                    return exhaustive::Pat::Wild;
                };
                self.expect_pat_ty(&adt, ty, range);
                let fields = self.tcx.struct_fields(def).unwrap_or_default();
                let map = [(def, args)];
                let mut subs: Vec<exhaustive::Pat> =
                    fields.iter().map(|_| exhaustive::Pat::Wild).collect();
                let mut seen = vec![false; fields.len()];
                for field in record.fields() {
                    let Some(name) = field.field_name() else {
                        continue;
                    };
                    let field_range = field.syntax().text_range();
                    let Some(position) = fields.iter().position(|(f, _)| *f == name) else {
                        self.pat_error = true;
                        self.no_field(field_range, &name, &adt);
                        continue;
                    };
                    let field_ty = fields
                        .get(position)
                        .map(|(_, field_ty)| subst(field_ty, &map))
                        .unwrap_or(Ty::Error);
                    let sub = match field.pat() {
                        Some(sub) => self.check_pat(&sub, &field_ty, binds),
                        None => exhaustive::Pat::Wild,
                    };
                    if let Some(slot) = subs.get_mut(position) {
                        *slot = sub;
                    }
                    if let Some(flag) = seen.get_mut(position) {
                        *flag = true;
                    }
                }
                if record.rest().is_none() {
                    let missing: Vec<String> = fields
                        .iter()
                        .zip(&seen)
                        .filter(|(_, seen)| !**seen)
                        .map(|((name, _), _)| format!("`{name}`"))
                        .collect();
                    if !missing.is_empty() {
                        return self.bad_pattern(
                            range,
                            &format!(
                                "pattern does not mention field(s) {}; add `..` to ignore them",
                                missing.join(", ")
                            ),
                        );
                    }
                }
                exhaustive::Pat::Struct(subs)
            }
            ast::Pat::PathPat(path_pat) => {
                let Some(path) = path_pat.path() else {
                    self.pat_error = true;
                    return exhaustive::Pat::Wild;
                };
                match self.resolve_value(&path, true) {
                    Some(ValueRes::Variant {
                        enum_def,
                        index,
                        args,
                    }) => self.unit_variant_pat(enum_def, index, args, ty, range),
                    Some(ValueRes::Def { def, impl_args, .. }) => {
                        match self.tcx.item(def).map(|item| &item.kind) {
                            Some(ItemKind::Struct {
                                fields: None,
                                generics,
                            }) => {
                                let args =
                                    impl_args.unwrap_or_else(|| self.fresh_n(generics.len()));
                                self.expect_pat_ty(&Ty::Adt { def, args }, ty, range);
                                exhaustive::Pat::Struct(Vec::new())
                            }
                            Some(ItemKind::Const) => {
                                self.bad_pattern(range, "constants cannot be used as patterns")
                            }
                            _ => self.bad_pattern(range, "expected a unit variant"),
                        }
                    }
                    Some(ValueRes::Local(_)) => self.bad_pattern(range, "expected a unit variant"),
                    None => {
                        self.pat_error = true;
                        exhaustive::Pat::Wild
                    }
                }
            }
            ast::Pat::OrPat(or) => {
                let mut alternatives = Vec::new();
                let mut first_names: Option<Vec<SmolStr>> = None;
                let mut first_binds = Vec::new();
                let mut mismatch = false;
                for alternative in or.pats() {
                    let mut alt_binds = Vec::new();
                    alternatives.push(self.check_pat(&alternative, ty, &mut alt_binds));
                    let mut names: Vec<SmolStr> =
                        alt_binds.iter().map(|(name, _)| name.clone()).collect();
                    names.sort();
                    match &first_names {
                        None => {
                            first_names = Some(names);
                            first_binds = alt_binds;
                        }
                        Some(first) => {
                            if *first != names {
                                mismatch = true;
                            } else {
                                for (name, alt_ty) in &alt_binds {
                                    if let Some((_, first_ty)) =
                                        first_binds.iter().find(|(first, _)| first == name)
                                    {
                                        let first_ty = first_ty.clone();
                                        self.expect_pat_ty(alt_ty, &first_ty, range);
                                    }
                                }
                            }
                        }
                    }
                }
                binds.extend(first_binds);
                if mismatch {
                    return self.bad_pattern(range, "all alternatives must bind the same names");
                }
                exhaustive::Pat::Or(alternatives)
            }
        }
    }

    fn binding(
        &mut self,
        name: SmolStr,
        ty: &Ty,
        range: TextRange,
        binds: &mut Vec<(SmolStr, Ty)>,
    ) -> exhaustive::Pat {
        if binds.iter().any(|(bound, _)| *bound == name) {
            return self.bad_pattern(range, &format!("`{name}` is bound more than once"));
        }
        binds.push((name, ty.clone()));
        exhaustive::Pat::Wild
    }

    fn unit_variant_pat(
        &mut self,
        enum_def: DefId,
        index: u32,
        args: Vec<Ty>,
        ty: &Ty,
        range: TextRange,
    ) -> exhaustive::Pat {
        let arity = self
            .tcx
            .enum_variants(enum_def)
            .get(index as usize)
            .map(|(_, payload)| payload.len())
            .unwrap_or(0);
        if arity != 0 {
            return self.bad_pattern(range, "this variant has fields; match them with `(..)`");
        }
        self.expect_pat_ty(
            &Ty::Adt {
                def: enum_def,
                args,
            },
            ty,
            range,
        );
        exhaustive::Pat::Variant(index as usize, Vec::new())
    }

    fn literal_pat(&mut self, pat: &ast::LiteralPat, ty: &Ty) -> Option<LitPat> {
        let range = pat.syntax().text_range();
        let literal = pat.literal()?;
        match literal.kind()? {
            LiteralKind::Int => {
                let lit_ty = self.literal(&literal, pat.negative());
                let value = literal
                    .int_value()
                    .and_then(|value| i128::try_from(value).ok())?;
                self.expect_pat_ty(&lit_ty, ty, range);
                Some(LitPat::Int(if pat.negative() {
                    value.saturating_neg()
                } else {
                    value
                }))
            }
            LiteralKind::Bool(value) if !pat.negative() => {
                self.expect_pat_ty(&Ty::Bool, ty, range);
                Some(LitPat::Bool(value))
            }
            _ => {
                self.bad_pattern(range, "only integer and bool literals can be patterns");
                None
            }
        }
    }

    // ---- witnesses ------------------------------------------------------------------------

    fn format_witness(&self, witness: &Witness, ty: &Ty) -> String {
        match witness {
            Witness::Wild => "_".to_owned(),
            Witness::Bool(value) => value.to_string(),
            Witness::Int(lo, hi) if lo == hi => lo.to_string(),
            Witness::Int(lo, hi) => format!("{lo}..={hi}"),
            Witness::Variant(index, subs) => {
                let Ty::Adt { def, args } = ty else {
                    return "_".to_owned();
                };
                let variants = self.tcx.enum_variants(*def);
                let Some((name, payload)) = variants.get(*index) else {
                    return "_".to_owned();
                };
                let head = format!("{}::{name}", self.tcx.name(*def));
                if payload.is_empty() {
                    return head;
                }
                let map = [(*def, args.clone())];
                let parts: Vec<String> = payload
                    .iter()
                    .enumerate()
                    .map(|(position, field_ty)| match subs.get(position) {
                        Some(sub) => self.format_witness(sub, &subst(field_ty, &map)),
                        None => "_".to_owned(),
                    })
                    .collect();
                format!("{head}({})", parts.join(", "))
            }
            Witness::Struct(subs) => {
                let Ty::Adt { def, args } = ty else {
                    return "_".to_owned();
                };
                let name = self.tcx.name(*def);
                let Some(fields) = self.tcx.struct_fields(*def) else {
                    return name.to_string();
                };
                let map = [(*def, args.clone())];
                let parts: Vec<String> = fields
                    .iter()
                    .zip(subs)
                    .filter(|(_, sub)| !matches!(sub, Witness::Wild))
                    .map(|((field, field_ty), sub)| {
                        format!(
                            "{field}: {}",
                            self.format_witness(sub, &subst(field_ty, &map))
                        )
                    })
                    .collect();
                if parts.is_empty() {
                    format!("{name} {{ .. }}")
                } else {
                    format!("{name} {{ {}, .. }}", parts.join(", "))
                }
            }
        }
    }
}

enum LitPat {
    Int(i128),
    Bool(bool),
}

/// Whether a resolved type still has unknown parts (errors or unresolved variables).
fn contains_unknown(ty: &Ty) -> bool {
    match ty {
        Ty::Error | Ty::Var(_) => true,
        Ty::Adt { args, .. } => args.iter().any(contains_unknown),
        _ => false,
    }
}

/// Type shapes for the exhaustiveness checker; types are fully resolved.
struct ExTypes<'a, 'db> {
    tcx: &'a TyCtx<'db>,
}

impl Types for ExTypes<'_, '_> {
    type Ty = Ty;

    fn kind(&self, ty: &Ty) -> TyKind<Ty> {
        match ty {
            Ty::Bool => TyKind::Bool,
            Ty::Int(int) => {
                let (min, max) = IntTy::range(*int);
                TyKind::Int { min, max }
            }
            Ty::Adt { def, args } => {
                let map = [(*def, args.clone())];
                match self.tcx.item(*def).map(|item| &item.kind) {
                    Some(ItemKind::Enum { .. }) => TyKind::Enum(
                        self.tcx
                            .enum_variants(*def)
                            .iter()
                            .map(|(_, payload)| payload.iter().map(|ty| subst(ty, &map)).collect())
                            .collect(),
                    ),
                    Some(ItemKind::Struct { .. }) => TyKind::Struct(
                        self.tcx
                            .struct_fields(*def)
                            .map(|fields| fields.iter().map(|(_, ty)| subst(ty, &map)).collect())
                            .unwrap_or_default(),
                    ),
                    _ => TyKind::Opaque,
                }
            }
            _ => TyKind::Opaque,
        }
    }
}
