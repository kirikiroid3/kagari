//! Lowering one instance to instructions: registers for parameters and locals, cells for
//! the locals whose place is taken, expressions, places, patterns and calls. The
//! interpreter (`eval::interp`) is the reference semantics.

use kagari_bytecode::{
    ArithOp, BoolBinOp, CmpOp, FnKind as BcFnKind, Instr, IntBinOp, IntType, IntUnOp, Label,
    RandDomain, Reg,
};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::def_map::{Def, Resolution};
use crate::eval::interp::{BodyRef, GenericEnv, subst_arg, subst_const, subst_fn_ref, subst_ty};
use crate::eval::std_items::Builtin;
use crate::eval::{ConstValue, Decimal, Text};
use crate::hir::{
    ArrayExpr, BinaryOp, ClosureKind, Condition, Expr, ExprId, FnPtrKind, Literal, Pat, PatBound,
    PatId, Stmt, UnaryOp,
};
use crate::infer::ValueRef;
use crate::infer::walk::{expr_children, pat_children};
use crate::item_tree::{ItemId, ItemKind, item_tree};
use crate::resolve::LocalId;
use crate::ty::{Const, CtorRef, FnDefId, FnRef, Ty};

use super::{Emitter, Instance, contains_screen, int_type_of};

/// A lowered function.
pub(super) struct Lowered {
    pub code: Vec<Instr>,
    pub regs: u16,
    /// More registers than a function can have.
    pub too_large: bool,
    /// `(instruction index, byte offset in the source file)`.
    pub lines: Vec<(u32, u32)>,
}

/// A register while lowering; converted to a `Reg` when an instruction is made.
type R = u32;

fn reg(r: R) -> Reg {
    u16::try_from(r).unwrap_or(u16::MAX)
}

fn count(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

fn strip_refs(ty: &Ty) -> &Ty {
    let mut ty = ty;
    while let Ty::Ref { inner, .. } = ty {
        ty = inner;
    }
    ty
}

fn is_ref(ty: Option<&Ty>) -> bool {
    matches!(ty, Some(Ty::Ref { .. }))
}

fn int_op(op: BinaryOp) -> Option<IntBinOp> {
    Some(match op {
        BinaryOp::Add => IntBinOp::Add,
        BinaryOp::Sub => IntBinOp::Sub,
        BinaryOp::Mul => IntBinOp::Mul,
        BinaryOp::Div => IntBinOp::Div,
        BinaryOp::Rem => IntBinOp::Rem,
        BinaryOp::BitAnd => IntBinOp::And,
        BinaryOp::BitOr => IntBinOp::Or,
        BinaryOp::BitXor => IntBinOp::Xor,
        BinaryOp::Shl => IntBinOp::Shl,
        BinaryOp::Shr => IntBinOp::Shr,
        _ => return None,
    })
}

fn arith_op(op: BinaryOp) -> Option<ArithOp> {
    Some(match op {
        BinaryOp::Add => ArithOp::Add,
        BinaryOp::Sub => ArithOp::Sub,
        BinaryOp::Mul => ArithOp::Mul,
        BinaryOp::Div => ArithOp::Div,
        _ => return None,
    })
}

fn bool_op(op: BinaryOp) -> Option<BoolBinOp> {
    Some(match op {
        BinaryOp::BitAnd => BoolBinOp::And,
        BinaryOp::BitOr => BoolBinOp::Or,
        BinaryOp::BitXor => BoolBinOp::Xor,
        _ => return None,
    })
}

fn cmp_op(op: BinaryOp) -> Option<CmpOp> {
    Some(match op {
        BinaryOp::Eq => CmpOp::Eq,
        BinaryOp::Ne => CmpOp::Ne,
        BinaryOp::Lt => CmpOp::Lt,
        BinaryOp::Le => CmpOp::Le,
        BinaryOp::Gt => CmpOp::Gt,
        BinaryOp::Ge => CmpOp::Ge,
        _ => return None,
    })
}

/// The value of a literal (negated for patterns).
fn literal_value(literal: &Literal, negated: bool) -> Option<ConstValue> {
    Some(match literal {
        Literal::Int { value, .. } => {
            let value = i128::try_from(*value).ok()?;
            ConstValue::Int(if negated { value.checked_neg()? } else { value })
        }
        Literal::Decimal(text) => {
            let value = Decimal::parse(text)?;
            ConstValue::Decimal(if negated { value.checked_neg()? } else { value })
        }
        Literal::String(text) => ConstValue::Text(Text::plain(text.clone())),
        Literal::Bool(value) => ConstValue::Bool(*value),
    })
}

struct LoopFrame {
    expr: ExprId,
    brk: Label,
    cont: Label,
    dst: R,
}

struct Lower<'e, 'a, 'db> {
    em: &'e mut Emitter<'a, 'db>,
    b: Option<BodyRef<'db>>,
    env: GenericEnv,
    /// The instance that owns the closures this function creates.
    root: Instance,
    /// A non-generic `pushdown fn` item: its folded constant subexpressions are used.
    fold: Option<ItemId>,
    code: Vec<Instr>,
    labels: Vec<Option<u32>>,
    locals: FxHashMap<LocalId, R>,
    celled: FxHashSet<LocalId>,
    next: R,
    high: R,
    loops: Vec<LoopFrame>,
    unreachable: Option<Label>,
    lines: Vec<(u32, u32)>,
    pending: Option<u32>,
}

pub(super) fn lower(em: &mut Emitter<'_, '_>, instance: &Instance) -> Lowered {
    let (kind_params, b, env, root) = match instance {
        Instance::Fn(fn_ref) => {
            let item = match fn_ref.def {
                FnDefId::Item(item) => Some(item),
                _ => None,
            };
            (
                item,
                item.and_then(|item| em.ctx.body(item)),
                em.instance_env(instance),
                instance.clone(),
            )
        }
        Instance::Closure { owner, .. } => (
            None,
            em.instance_item(owner).and_then(|item| em.ctx.body(item)),
            em.instance_env(owner),
            (**owner).clone(),
        ),
        Instance::ConstInit(item, _) => (
            None,
            em.ctx.body(*item),
            em.instance_env(instance),
            instance.clone(),
        ),
        Instance::Ctor(_) | Instance::Builtin(_) => {
            (None, None, em.instance_env(instance), instance.clone())
        }
    };
    let fold = kind_params.filter(|item| {
        em.sigs.fn_sig(*item).is_some_and(|sig| {
            sig.kind == crate::item_tree::FnKind::Pushdown
                && sig.generics.params.is_empty()
                && sig.generics.parent_count == 0
        })
    });
    let mut lower = Lower {
        em,
        b,
        env,
        root,
        fold,
        code: Vec::new(),
        labels: Vec::new(),
        locals: FxHashMap::default(),
        celled: FxHashSet::default(),
        next: 0,
        high: 0,
        loops: Vec::new(),
        unreachable: None,
        lines: Vec::new(),
        pending: None,
    };
    match instance {
        Instance::Fn(_) | Instance::ConstInit(..) => lower.item_body(instance),
        Instance::Closure { expr, .. } => lower.closure_body(*expr),
        Instance::Ctor(fn_ref) => lower.ctor_body(fn_ref),
        Instance::Builtin(fn_ref) => lower.builtin_body(fn_ref),
    }
    lower.finish()
}

impl<'db> Lower<'_, '_, 'db> {
    // ----- Emission -----------------------------------------------------------------

    fn emit(&mut self, instr: Instr) {
        if let Some(offset) = self.pending.take()
            && self.lines.last().is_none_or(|(_, last)| *last != offset)
        {
            self.lines
                .push((u32::try_from(self.code.len()).unwrap_or(u32::MAX), offset));
        }
        self.code.push(instr);
    }

    fn label(&mut self) -> Label {
        self.labels.push(None);
        Label(u32::try_from(self.labels.len().saturating_sub(1)).unwrap_or(u32::MAX))
    }

    fn bind(&mut self, label: Label) {
        let here = u32::try_from(self.code.len()).unwrap_or(u32::MAX);
        if let Some(slot) = self.labels.get_mut(label.0 as usize) {
            *slot = Some(here);
        }
    }

    fn temp(&mut self) -> R {
        let r = self.next;
        self.next = self.next.saturating_add(1);
        self.high = self.high.max(self.next);
        r
    }

    fn temps(&mut self, n: usize) -> R {
        let first = self.next;
        self.next = self
            .next
            .saturating_add(u32::try_from(n).unwrap_or(u32::MAX));
        self.high = self.high.max(self.next);
        first
    }

    fn nth(first: R, index: usize) -> R {
        first.saturating_add(u32::try_from(index).unwrap_or(u32::MAX))
    }

    fn mov(&mut self, dst: R, src: R) {
        if dst != src {
            self.emit(Instr::Move {
                dst: reg(dst),
                src: reg(src),
            });
        }
    }

    fn unit(&mut self, dst: R) {
        self.emit(Instr::Tuple {
            dst: reg(dst),
            first: 0,
            count: 0,
        });
    }

    fn jmp(&mut self, target: Label) {
        self.emit(Instr::Jmp { target });
    }

    /// The label of the single `UNREACHABLE` irrefutable patterns fail to.
    fn unreachable_label(&mut self) -> Label {
        match self.unreachable {
            Some(label) => label,
            None => {
                let label = self.label();
                self.unreachable = Some(label);
                label
            }
        }
    }

    fn finish(mut self) -> Lowered {
        if let Some(label) = self.unreachable
            && self.code.iter().any(|instr| instr.target() == Some(label))
        {
            self.bind(label);
            self.emit(Instr::Unreachable);
        }
        let labels = self.labels;
        let target =
            |label: Label| Label(labels.get(label.0 as usize).copied().flatten().unwrap_or(0));
        let code = self
            .code
            .into_iter()
            .map(|instr| match instr {
                Instr::Jmp { target: label } => Instr::Jmp {
                    target: target(label),
                },
                Instr::JmpIf {
                    cond,
                    target: label,
                } => Instr::JmpIf {
                    cond,
                    target: target(label),
                },
                Instr::JmpNot {
                    cond,
                    target: label,
                } => Instr::JmpNot {
                    cond,
                    target: target(label),
                },
                instr => instr,
            })
            .collect();
        Lowered {
            code,
            regs: reg(self.high),
            too_large: self.high > u32::from(u16::MAX),
            lines: self.lines,
        }
    }

    // ----- Context ------------------------------------------------------------------

    fn body(&self) -> Option<BodyRef<'db>> {
        self.b
    }

    fn expr_data(&self, expr: ExprId) -> Option<&'db Expr> {
        Some(self.b?.body.store.expr(expr))
    }

    fn ty(&self, expr: ExprId) -> Ty {
        match self.b.and_then(|b| b.infer.expr_types.get(expr)) {
            Some(ty) => subst_ty(ty, &self.env),
            None => Ty::Error,
        }
    }

    fn raw_ty(&self, expr: ExprId) -> Option<&'db Ty> {
        self.b?.infer.expr_types.get(expr)
    }

    fn local_ty(&self, local: LocalId) -> Option<&'db Ty> {
        self.b?.infer.local_types.get(local)
    }

    fn pat_ty(&self, pat: PatId) -> Option<&'db Ty> {
        self.b?.infer.pat_types.get(pat)
    }

    fn local_reg(&mut self, local: LocalId) -> R {
        if let Some(r) = self.locals.get(&local) {
            return *r;
        }
        // A local this function never bound (an inference gap): give it a register so the
        // code stays well-formed; reading it panics as Uninit.
        let r = self.temp();
        self.locals.insert(local, r);
        r
    }

    fn is_static_mut(&self, item: ItemId) -> bool {
        matches!(
            item_tree(self.em.db, item.file)
                .item(item.index)
                .map(|data| &data.kind),
            Some(ItemKind::Static { mutable: true })
        )
    }

    // ----- Function bodies ----------------------------------------------------------

    /// Collects the locals bound in the subtree of `exprs` and `pats` (not inside nested
    /// closures), in order, and which of them need a cell.
    fn collect(&mut self, exprs: Vec<ExprId>, pats: Vec<PatId>) -> Vec<LocalId> {
        let Some(b) = self.b else {
            return Vec::new();
        };
        let store = &b.body.store;
        let mut bound = Vec::new();
        let mut expr_stack = exprs;
        expr_stack.reverse();
        let mut pat_stack = pats;
        pat_stack.reverse();
        loop {
            if let Some(pat) = pat_stack.pop() {
                let data = store.pat(pat);
                if matches!(data, Pat::Bind { .. })
                    && !b.infer.pat_ctors.contains_key(&pat)
                    && let Some(local) = b.res.bindings.get(&pat)
                    && !bound.contains(local)
                {
                    bound.push(*local);
                }
                let mut children = Vec::new();
                pat_children(data, &mut |child| children.push(child));
                pat_stack.extend(children.into_iter().rev());
                continue;
            }
            let Some(expr) = expr_stack.pop() else {
                break;
            };
            let data = store.expr(expr);
            match data {
                Expr::Closure { .. } => continue,
                Expr::Ref { expr: inner, .. } => self.mark_root(*inner),
                Expr::MethodCall { receiver, .. } => {
                    if b.infer.method_calls.get(&expr).is_some_and(|call| {
                        call.receiver.autoref.is_some() && call.receiver.derefs == 0
                    }) {
                        self.mark_root(*receiver);
                    }
                }
                Expr::Assign { target, .. } => {
                    let bare = matches!(store.expr(*target), Expr::Path(_))
                        && matches!(b.infer.paths.get(target), Some(ValueRef::Local(_)));
                    if !bare {
                        self.mark_root(*target);
                    }
                }
                _ => {}
            }
            let mut children = Vec::new();
            let mut pats = Vec::new();
            expr_children(data, &mut |child| children.push(child), &mut |pat| {
                pats.push(pat)
            });
            expr_stack.extend(children.into_iter().rev());
            pat_stack.extend(pats.into_iter().rev());
        }
        bound
    }

    /// Marks the local at the root of the place `expr` as celled: `Field` and `Index`
    /// without a dereference are stripped down to a local path.
    fn mark_root(&mut self, expr: ExprId) {
        let Some(b) = self.b else {
            return;
        };
        let mut expr = expr;
        loop {
            match b.body.store.expr(expr) {
                Expr::Field { base, .. } => {
                    if b.infer
                        .fields
                        .get(&expr)
                        .is_some_and(|field| field.derefs == 0)
                    {
                        expr = *base;
                    } else {
                        return;
                    }
                }
                Expr::Index { base, .. } => {
                    if is_ref(b.infer.expr_types.get(*base)) {
                        return;
                    }
                    expr = *base;
                }
                Expr::Path(_) => {
                    if let Some(ValueRef::Local(local)) = b.infer.paths.get(&expr)
                        && !is_ref(b.infer.local_types.get(*local))
                    {
                        self.celled.insert(*local);
                    }
                    return;
                }
                _ => return,
            }
        }
    }

    /// Gives every collected local a register, except those already aliased.
    fn allocate(&mut self, locals: &[LocalId]) {
        for local in locals {
            if !self.locals.contains_key(local) {
                let r = self.temp();
                self.locals.insert(*local, r);
            }
        }
    }

    /// Binds the parameter pattern `pat` held in register `param`: a plain binding uses the
    /// register itself.
    fn param_alias(&mut self, pat: PatId, param: R) -> bool {
        let Some(b) = self.b else {
            return false;
        };
        if !matches!(b.body.store.pat(pat), Pat::Bind { .. })
            || b.infer.pat_ctors.contains_key(&pat)
        {
            return false;
        }
        let Some(local) = b.res.bindings.get(&pat).copied() else {
            return false;
        };
        if self.celled.contains(&local) {
            return false;
        }
        self.locals.insert(local, param);
        true
    }

    fn item_body(&mut self, instance: &Instance) {
        let Some(b) = self.b else {
            self.emit(Instr::Unreachable);
            return;
        };
        let params: Vec<PatId> = match instance {
            Instance::Fn(_) => b.body.params.clone(),
            _ => Vec::new(),
        };
        let has_self = matches!(instance, Instance::Fn(_)) && b.body.has_self;
        let locals = self.collect(vec![b.body.value], params.clone());
        let self_local = has_self
            .then(|| {
                b.res
                    .locals
                    .iter()
                    .find(|(_, local)| local.pat.is_none())
                    .map(|(id, _)| id)
            })
            .flatten();
        let offset = usize::from(has_self);
        self.next = u32::try_from(params.len().saturating_add(offset)).unwrap_or(u32::MAX);
        self.high = self.next;
        let self_celled = self_local.is_some_and(|local| self.celled.contains(&local));
        if let Some(local) = self_local
            && !self_celled
        {
            self.locals.insert(local, 0);
        }
        let mut pending = Vec::new();
        for (index, pat) in params.iter().enumerate() {
            let param = Self::nth(0, index.saturating_add(offset));
            if !self.param_alias(*pat, param) {
                pending.push((*pat, param));
            }
        }
        self.allocate(&locals);
        if let Some(local) = self_local
            && self_celled
        {
            let r = self.temp();
            self.locals.insert(local, r);
            self.emit(Instr::Cell {
                dst: reg(r),
                init: 0,
            });
        }
        for (pat, param) in pending {
            self.irrefutable(pat, param);
        }
        let ret = self.temp();
        self.expr(b.body.value, ret);
        self.emit(Instr::Ret { src: reg(ret) });
    }

    fn closure_body(&mut self, closure: ExprId) {
        let Some(b) = self.b else {
            self.emit(Instr::Unreachable);
            return;
        };
        let Expr::Closure { params, body, .. } = b.body.store.expr(closure) else {
            self.emit(Instr::Unreachable);
            return;
        };
        let captures = b
            .infer
            .closures
            .get(&closure)
            .map(|info| info.captures.clone())
            .unwrap_or_default();
        let pats: Vec<PatId> = params.iter().map(|(pat, _)| *pat).collect();
        let locals = self.collect(vec![*body], pats.clone());
        self.next = u32::try_from(captures.len().saturating_add(pats.len())).unwrap_or(u32::MAX);
        self.high = self.next;
        let mut celled_captures = Vec::new();
        for (index, local) in captures.iter().enumerate() {
            let r = Self::nth(0, index);
            if self.celled.contains(local) {
                celled_captures.push((*local, r));
            } else {
                self.locals.insert(*local, r);
            }
        }
        let mut pending = Vec::new();
        for (index, pat) in pats.iter().enumerate() {
            let param = Self::nth(0, index.saturating_add(captures.len()));
            if !self.param_alias(*pat, param) {
                pending.push((*pat, param));
            }
        }
        self.allocate(&locals);
        for (local, capture) in celled_captures {
            let r = self.temp();
            self.locals.insert(local, r);
            self.emit(Instr::Cell {
                dst: reg(r),
                init: reg(capture),
            });
        }
        for (pat, param) in pending {
            self.irrefutable(pat, param);
        }
        let ret = self.temp();
        self.expr(*body, ret);
        self.emit(Instr::Ret { src: reg(ret) });
    }

    fn ctor_body(&mut self, fn_ref: &FnRef) {
        let (item, variant) = match fn_ref.def {
            FnDefId::StructCtor(item) => (item, 0),
            FnDefId::VariantCtor(item, variant) => (item, variant),
            FnDefId::Item(_) => {
                self.emit(Instr::Unreachable);
                return;
            }
        };
        let (_, params) = self.em.instance_shape(&Instance::Ctor(fn_ref.clone()));
        self.next = u32::from(params);
        let dst = self.temp();
        let ty = self.em.type_id(item);
        self.emit(Instr::Adt {
            dst: reg(dst),
            first: 0,
            count: params,
            ty,
            variant,
        });
        self.emit(Instr::Ret { src: reg(dst) });
    }

    fn builtin_body(&mut self, fn_ref: &FnRef) {
        let FnDefId::Item(item) = fn_ref.def else {
            self.emit(Instr::Unreachable);
            return;
        };
        let Some(sig) = self.em.sigs.fn_sig(item) else {
            self.emit(Instr::Unreachable);
            return;
        };
        let params = sig.params.len();
        let ret_ty = subst_ty(&sig.ret, &self.env);
        self.next = u32::try_from(params).unwrap_or(u32::MAX);
        self.high = self.next;
        let dst = self.temp();
        match self.em.std.builtin(item) {
            Some(builtin) => self.builtin(item, builtin, fn_ref, 0, params, dst, &ret_ty, None),
            None => {
                let import = self.em.import(item);
                self.emit(Instr::CallImp {
                    dst: reg(dst),
                    args: 0,
                    argc: count(params),
                    import,
                });
            }
        }
        self.emit(Instr::Ret { src: reg(dst) });
    }

    // ----- Expressions --------------------------------------------------------------

    fn value(&mut self, expr: ExprId) -> R {
        let t = self.temp();
        self.expr(expr, t);
        t
    }

    /// The register of a plain local read by `expr`, when no expression evaluated after it
    /// (`later`) can assign it; else a temporary holding the value.
    fn operand(&mut self, expr: ExprId, later: &[ExprId]) -> R {
        if let Some(b) = self.b
            && matches!(b.body.store.expr(expr), Expr::Path(_))
            && let Some(ValueRef::Local(local)) = b.infer.paths.get(&expr)
            && !self.celled.contains(local)
            && let Some(r) = self.locals.get(local).copied()
            && !later.iter().any(|later| self.assigns(*later))
        {
            self.pending = Some(u32::from(b.body.store.expr_range(expr).start()));
            return r;
        }
        self.value(expr)
    }

    /// Whether `expr` contains an assignment (outside nested closures).
    fn assigns(&self, expr: ExprId) -> bool {
        let Some(b) = self.b else {
            return false;
        };
        let mut stack = vec![expr];
        while let Some(expr) = stack.pop() {
            let data = b.body.store.expr(expr);
            match data {
                Expr::Assign { .. } => return true,
                Expr::Closure { .. } => continue,
                _ => expr_children(data, &mut |child| stack.push(child), &mut |_| {}),
            }
        }
        false
    }

    fn load_value(&mut self, dst: R, value: &ConstValue) {
        match value {
            ConstValue::Bool(value) => self.emit(Instr::Bool {
                dst: reg(dst),
                value: *value,
            }),
            ConstValue::Int(value) if i64::try_from(*value).is_ok() => self.emit(Instr::Int {
                dst: reg(dst),
                value: i64::try_from(*value).unwrap_or(0),
            }),
            _ => match self.em.constant(value) {
                Some(id) => self.emit(Instr::Const {
                    dst: reg(dst),
                    value: id,
                }),
                None => self.emit(Instr::Unreachable),
            },
        }
    }

    /// The value of the `const` item `item` with parent arguments `args`.
    fn const_item(&mut self, item: ItemId, args: Vec<crate::ty::GenericArg>, dst: R, at: ExprId) {
        match self.em.ctx.const_value(item, args.clone()) {
            Ok(value) if !contains_screen(&value) => self.load_value(dst, &value),
            Ok(_) => {
                let func = self.em.request(Instance::ConstInit(item, args));
                self.emit(Instr::Call {
                    dst: reg(dst),
                    args: 0,
                    argc: 0,
                    func,
                });
            }
            Err(error) => {
                self.eval_error(&error, at);
                self.emit(Instr::Unreachable);
            }
        }
    }

    /// A const-evaluation error found while emitting (a generic constant instantiated
    /// only by run-time code).
    fn eval_error(&mut self, error: &crate::eval::interp::EvalError, at: ExprId) {
        let crate::eval::interp::ErrorKind::Fault(fault) = &error.kind else {
            return;
        };
        let (file, range) = match (error.span, self.b) {
            (Some(span), _) => (span.file.path(self.em.db).clone(), span.range),
            (None, Some(b)) => (
                b.item.file.path(self.em.db).clone(),
                b.body.store.expr_range(at),
            ),
            (None, None) => return,
        };
        self.em.diagnostics.push(crate::diagnostic::Diagnostic::new(
            file,
            range,
            fault.code,
            fault.message.clone(),
        ));
    }

    fn expr(&mut self, expr: ExprId, dst: R) {
        let Some(b) = self.body() else {
            self.emit(Instr::Unreachable);
            return;
        };
        self.pending = Some(u32::from(b.body.store.expr_range(expr).start()));
        if let Some(item) = self.fold
            && let Some(value) = self.em.eval.folded.get(&(item, expr))
            && !contains_screen(value)
        {
            let value = value.clone();
            self.load_value(dst, &value);
            return;
        }
        match b.body.store.expr(expr) {
            Expr::Missing | Expr::Cast { .. } => self.emit(Instr::Unreachable),
            Expr::Literal(literal) => match literal_value(literal, false) {
                Some(value) => self.load_value(dst, &value),
                None => self.emit(Instr::Unreachable),
            },
            Expr::Path(_) => self.path(expr, dst),
            Expr::Block { stmts, tail, .. } => {
                for stmt in stmts {
                    let saved = self.next;
                    self.stmt(stmt);
                    self.next = saved;
                }
                match tail {
                    Some(tail) => self.expr(*tail, dst),
                    None => self.unit(dst),
                }
            }
            Expr::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let other = self.label();
                let end = self.label();
                self.condition(cond, other);
                self.expr(*then_branch, dst);
                self.jmp(end);
                self.bind(other);
                match else_branch {
                    Some(branch) => self.expr(*branch, dst),
                    None => self.unit(dst),
                }
                self.bind(end);
            }
            Expr::Match { scrutinee, arms } => {
                let scrut = self.value(*scrutinee);
                let end = self.label();
                for arm in arms {
                    let next = self.label();
                    let place = self.place_mode(arm.pat, false);
                    self.pat(arm.pat, scrut, place, next);
                    if let Some(guard) = arm.guard {
                        let g = self.value(guard);
                        self.emit(Instr::JmpNot {
                            cond: reg(g),
                            target: next,
                        });
                    }
                    self.expr(arm.body, dst);
                    self.jmp(end);
                    self.bind(next);
                }
                self.emit(Instr::Unreachable);
                self.bind(end);
            }
            Expr::Loop { body, .. } => {
                let head = self.label();
                let exit = self.label();
                self.bind(head);
                self.loops.push(LoopFrame {
                    expr,
                    brk: exit,
                    cont: head,
                    dst,
                });
                let t = self.temp();
                self.expr(*body, t);
                self.jmp(head);
                self.loops.pop();
                self.bind(exit);
            }
            Expr::While { cond, body, .. } => {
                let head = self.label();
                let exit = self.label();
                self.bind(head);
                self.condition(cond, exit);
                let t = self.temp();
                self.loops.push(LoopFrame {
                    expr,
                    brk: exit,
                    cont: head,
                    dst: t,
                });
                self.expr(*body, t);
                self.jmp(head);
                self.loops.pop();
                self.bind(exit);
                self.unit(dst);
            }
            Expr::For {
                pat,
                iterable,
                body,
                ..
            } => self.for_loop(expr, *pat, *iterable, *body, dst),
            Expr::Break { value, .. } => {
                let target = b.res.loop_targets.get(&expr).copied();
                let frame = self
                    .loops
                    .iter()
                    .rev()
                    .find(|frame| Some(frame.expr) == target)
                    .map(|frame| (frame.brk, frame.dst));
                let Some((brk, frame_dst)) = frame else {
                    self.emit(Instr::Unreachable);
                    return;
                };
                match value {
                    Some(value) => self.expr(*value, frame_dst),
                    None => self.unit(frame_dst),
                }
                self.jmp(brk);
            }
            Expr::Continue { .. } => {
                let target = b.res.loop_targets.get(&expr).copied();
                let cont = self
                    .loops
                    .iter()
                    .rev()
                    .find(|frame| Some(frame.expr) == target)
                    .map(|frame| frame.cont);
                match cont {
                    Some(cont) => self.jmp(cont),
                    None => self.emit(Instr::Unreachable),
                }
            }
            Expr::Return { value } => {
                let t = self.temp();
                match value {
                    Some(value) => self.expr(*value, t),
                    None => self.unit(t),
                }
                self.emit(Instr::Ret { src: reg(t) });
            }
            Expr::Call { callee, args } => self.call_expr(expr, *callee, args, dst),
            Expr::MethodCall { receiver, args, .. } => {
                self.method_call(expr, *receiver, args, dst);
            }
            Expr::Field { base, .. } => {
                let index = b.infer.fields.get(&expr).map_or(0, |field| field.index);
                let base = self.operand(*base, &[]);
                self.emit(Instr::Field {
                    dst: reg(dst),
                    src: reg(base),
                    index: u16::try_from(index).unwrap_or(u16::MAX),
                });
            }
            Expr::Index { base, index } => {
                let base = self.operand(*base, &[*index]);
                let index = self.operand(*index, &[]);
                self.emit(Instr::Index {
                    dst: reg(dst),
                    base: reg(base),
                    index: reg(index),
                });
            }
            Expr::Try(inner) => self.try_expr(*inner, dst),
            Expr::Unary { op, expr: inner } => self.unary(expr, *op, *inner, dst),
            Expr::Ref { expr: inner, .. } => {
                let place = self.place(*inner);
                self.mov(dst, place);
            }
            Expr::Binary { op, lhs, rhs } => self.binary(expr, *op, *lhs, *rhs, dst),
            Expr::Assign { op, target, value } => {
                self.assign(*op, *target, *value);
                self.unit(dst);
            }
            Expr::Range {
                start,
                end,
                inclusive,
            } => {
                let range = self.em.std.range.clone();
                let first = self.temps(3);
                for (name, value) in [("start", start), ("end", end)] {
                    let Some(index) = range.field(name) else {
                        continue;
                    };
                    match value {
                        Some(value) => self.expr(*value, Self::nth(first, index)),
                        None => self.emit(Instr::Unreachable),
                    }
                }
                if let Some(index) = range.field("inclusive") {
                    self.emit(Instr::Bool {
                        dst: reg(Self::nth(first, index)),
                        value: *inclusive,
                    });
                }
                let ty = self.em.type_id(range.item);
                self.emit(Instr::Adt {
                    dst: reg(dst),
                    first: reg(first),
                    count: 3,
                    ty,
                    variant: 0,
                });
            }
            Expr::Tuple(items) => {
                let first = self.list(items);
                self.emit(Instr::Tuple {
                    dst: reg(dst),
                    first: reg(first),
                    count: count(items.len()),
                });
            }
            Expr::Array(ArrayExpr::List(items)) => {
                let first = self.list(items);
                self.emit(Instr::Array {
                    dst: reg(dst),
                    first: reg(first),
                    count: count(items.len()),
                });
            }
            Expr::Array(ArrayExpr::Repeat { value, .. }) => {
                let len = match self.ty(expr) {
                    Ty::Array {
                        len: Const::Value(len),
                        ..
                    } => u64::try_from(len).unwrap_or(0),
                    _ => 0,
                };
                let value = self.value(*value);
                self.emit(Instr::Repeat {
                    dst: reg(dst),
                    value: reg(value),
                    count: len,
                });
            }
            Expr::Record { fields, .. } => self.record(expr, fields, dst),
            Expr::Closure { .. } => self.closure(expr, dst),
        }
    }

    fn list(&mut self, items: &[ExprId]) -> R {
        let first = self.temps(items.len());
        for (index, item) in items.iter().enumerate() {
            self.expr(*item, Self::nth(first, index));
        }
        first
    }

    fn stmt(&mut self, stmt: &Stmt) {
        match *stmt {
            Stmt::Let { pat, init, .. } => match init {
                Some(init) => match self.plain_binding(pat) {
                    Some(local) => {
                        let r = self.local_reg(local);
                        self.expr(init, r);
                    }
                    None => {
                        let value = self.value(init);
                        self.irrefutable(pat, value);
                    }
                },
                None => self.declare(pat),
            },
            Stmt::Expr { expr, .. } => match self.expr_data(expr) {
                // An assignment statement: its unit value is never read.
                Some(Expr::Assign { op, target, value }) => self.assign(*op, *target, *value),
                _ => {
                    self.value(expr);
                }
            },
            Stmt::Item(_) => {}
        }
    }

    /// The local `pat` binds when it is a plain binding into a register.
    fn plain_binding(&self, pat: PatId) -> Option<LocalId> {
        let b = self.b?;
        if !matches!(b.body.store.pat(pat), Pat::Bind { .. })
            || b.infer.pat_ctors.contains_key(&pat)
        {
            return None;
        }
        let local = b.res.bindings.get(&pat).copied()?;
        (!self.celled.contains(&local)).then_some(local)
    }

    /// `let pat;`: every binding of `pat` becomes Uninit.
    fn declare(&mut self, pat: PatId) {
        let Some(b) = self.b else {
            return;
        };
        let mut stack = vec![pat];
        while let Some(pat) = stack.pop() {
            let data = b.body.store.pat(pat);
            if matches!(data, Pat::Bind { .. })
                && let Some(local) = b.res.bindings.get(&pat).copied()
            {
                let r = self.local_reg(local);
                if self.celled.contains(&local) {
                    let t = self.temp();
                    self.emit(Instr::Clear { reg: reg(t) });
                    self.emit(Instr::Cell {
                        dst: reg(r),
                        init: reg(t),
                    });
                } else {
                    self.emit(Instr::Clear { reg: reg(r) });
                }
            }
            pat_children(data, &mut |child| stack.push(child));
        }
    }

    /// Lowers a condition; jumps to `fail` when it does not hold.
    fn condition(&mut self, cond: &Condition, fail: Label) {
        match *cond {
            Condition::Expr(expr) => {
                let value = self.operand(expr, &[]);
                self.emit(Instr::JmpNot {
                    cond: reg(value),
                    target: fail,
                });
            }
            Condition::Let { pat, expr } => {
                let value = self.value(expr);
                let place = self.place_mode(pat, false);
                self.pat(pat, value, place, fail);
            }
        }
    }

    fn read_local(&mut self, local: LocalId, dst: R) {
        let r = self.local_reg(local);
        if self.celled.contains(&local) {
            self.emit(Instr::Load {
                dst: reg(dst),
                src: reg(r),
            });
        } else {
            self.mov(dst, r);
        }
    }

    fn path(&mut self, expr: ExprId, dst: R) {
        let Some(b) = self.b else {
            return;
        };
        let Some(path) = b.infer.paths.get(&expr) else {
            self.emit(Instr::Unreachable);
            return;
        };
        match path {
            ValueRef::Local(local) => self.read_local(*local, dst),
            ValueRef::Fn(fn_ref) => {
                let fn_ref = subst_fn_ref(fn_ref, &self.env);
                let func = self.em.func_value(&fn_ref);
                self.emit(Instr::Func {
                    dst: reg(dst),
                    func,
                });
            }
            ValueRef::Ctor(ctor) => self.unit_ctor(ctor, dst),
            ValueRef::Const { item, args } => {
                let args = args.iter().map(|arg| subst_arg(arg, &self.env)).collect();
                self.const_item(*item, args, dst, expr);
            }
            ValueRef::Static(item) => self.static_value(*item, dst, expr),
            ValueRef::ConstParam { owner, index } => {
                match subst_const(
                    &Const::Param {
                        owner: *owner,
                        index: *index,
                    },
                    &self.env,
                ) {
                    Const::Value(value) => self.load_value(dst, &ConstValue::Int(value)),
                    _ => self.emit(Instr::Unreachable),
                }
            }
            ValueRef::Error => self.emit(Instr::Unreachable),
        }
    }

    fn unit_ctor(&mut self, ctor: &CtorRef, dst: R) {
        let ty = self.em.type_id(ctor.adt);
        self.emit(Instr::Adt {
            dst: reg(dst),
            first: 0,
            count: 0,
            ty,
            variant: ctor.variant.unwrap_or(0),
        });
    }

    fn static_value(&mut self, item: ItemId, dst: R, at: ExprId) {
        if self.is_static_mut(item) {
            let place = self.static_place(item);
            self.emit(Instr::Load {
                dst: reg(dst),
                src: reg(place),
            });
            return;
        }
        match self.em.ctx.static_value(item) {
            Ok(value) if value.handle.is_some() => match self.em.statics.get(&item).copied() {
                Some(id) => {
                    let constant = self.em.intern(kagari_bytecode::Constant::Handle(id));
                    self.emit(Instr::Const {
                        dst: reg(dst),
                        value: constant,
                    });
                }
                None => self.emit(Instr::Unreachable),
            },
            Ok(value) if !contains_screen(&value.value) => self.load_value(dst, &value.value),
            Ok(_) => {
                let func = self.em.request(Instance::ConstInit(item, Vec::new()));
                self.emit(Instr::Call {
                    dst: reg(dst),
                    args: 0,
                    argc: 0,
                    func,
                });
            }
            Err(error) => {
                self.eval_error(&error, at);
                self.emit(Instr::Unreachable);
            }
        }
    }

    fn static_place(&mut self, item: ItemId) -> R {
        let t = self.temp();
        match self.em.statics.get(&item).copied() {
            Some(var) => self.emit(Instr::RefStatic { dst: reg(t), var }),
            None => self.emit(Instr::Unreachable),
        }
        t
    }

    fn for_loop(&mut self, expr: ExprId, pat: PatId, iterable: ExprId, body: ExprId, dst: R) {
        let iter_ty = self.ty(iterable);
        let range = self.em.std.range.clone();
        let it = self.value(iterable);
        let head = self.label();
        let cont = self.label();
        let exit = self.label();
        let one = self.temp();
        self.emit(Instr::Int {
            dst: reg(one),
            value: 1,
        });
        let i = self.temp();
        let c = self.temp();
        let int0 = self.em.int_type(IntType::I128);
        match strip_refs(&iter_ty) {
            Ty::Adt { item, .. } if *item == range.item => {
                let field = |name: &str| u16::try_from(range.field(name).unwrap_or(0)).unwrap_or(0);
                let end = self.temp();
                let incl = self.temp();
                self.emit(Instr::Field {
                    dst: reg(i),
                    src: reg(it),
                    index: field("start"),
                });
                self.emit(Instr::Field {
                    dst: reg(end),
                    src: reg(it),
                    index: field("end"),
                });
                self.emit(Instr::Field {
                    dst: reg(incl),
                    src: reg(it),
                    index: field("inclusive"),
                });
                // `inclusive` is known when the iterable is a range expression.
                let known = match self.expr_data(iterable) {
                    Some(Expr::Range { inclusive, .. }) => Some(*inclusive),
                    _ => None,
                };
                let enter = self.label();
                self.bind(head);
                match known {
                    Some(inclusive) => {
                        self.emit(Instr::Cmp {
                            op: if inclusive { CmpOp::Le } else { CmpOp::Lt },
                            dst: reg(c),
                            lhs: reg(i),
                            rhs: reg(end),
                        });
                        self.emit(Instr::JmpNot {
                            cond: reg(c),
                            target: exit,
                        });
                    }
                    None => {
                        self.emit(Instr::Cmp {
                            op: CmpOp::Lt,
                            dst: reg(c),
                            lhs: reg(i),
                            rhs: reg(end),
                        });
                        self.emit(Instr::JmpIf {
                            cond: reg(c),
                            target: enter,
                        });
                        self.emit(Instr::JmpNot {
                            cond: reg(incl),
                            target: exit,
                        });
                        self.emit(Instr::Cmp {
                            op: CmpOp::Eq,
                            dst: reg(c),
                            lhs: reg(i),
                            rhs: reg(end),
                        });
                        self.emit(Instr::JmpNot {
                            cond: reg(c),
                            target: exit,
                        });
                    }
                }
                self.bind(enter);
                self.loop_body(expr, pat, i, body, exit, cont);
                self.bind(cont);
                // An inclusive range stops at `end` before incrementing, so `..=MAX` cannot
                // overflow.
                let inc = self.label();
                match known {
                    Some(false) => {}
                    Some(true) => {
                        self.emit(Instr::Cmp {
                            op: CmpOp::Eq,
                            dst: reg(c),
                            lhs: reg(i),
                            rhs: reg(end),
                        });
                        self.emit(Instr::JmpIf {
                            cond: reg(c),
                            target: exit,
                        });
                    }
                    None => {
                        self.emit(Instr::JmpNot {
                            cond: reg(incl),
                            target: inc,
                        });
                        self.emit(Instr::Cmp {
                            op: CmpOp::Eq,
                            dst: reg(c),
                            lhs: reg(i),
                            rhs: reg(end),
                        });
                        self.emit(Instr::JmpIf {
                            cond: reg(c),
                            target: exit,
                        });
                    }
                }
                self.bind(inc);
            }
            _ => {
                // An array or a `Vec`: a copy of the value, iterated by index.
                let items = self.temp();
                if matches!(iter_ty, Ty::Ref { .. }) {
                    self.emit(Instr::Load {
                        dst: reg(items),
                        src: reg(it),
                    });
                } else {
                    self.mov(items, it);
                }
                let len = self.temp();
                self.emit(Instr::Len {
                    dst: reg(len),
                    src: reg(items),
                });
                self.emit(Instr::Int {
                    dst: reg(i),
                    value: 0,
                });
                self.bind(head);
                self.emit(Instr::Cmp {
                    op: CmpOp::Lt,
                    dst: reg(c),
                    lhs: reg(i),
                    rhs: reg(len),
                });
                self.emit(Instr::JmpNot {
                    cond: reg(c),
                    target: exit,
                });
                let item = self.temp();
                self.emit(Instr::Index {
                    dst: reg(item),
                    base: reg(items),
                    index: reg(i),
                });
                self.loop_body(expr, pat, item, body, exit, cont);
                self.bind(cont);
            }
        }
        self.emit(Instr::IntBin {
            op: IntBinOp::Add,
            dst: reg(i),
            lhs: reg(i),
            rhs: reg(one),
            ty: int0,
        });
        self.jmp(head);
        self.bind(exit);
        self.unit(dst);
    }

    /// Binds the loop pattern to `item` and runs the body of the `for` loop `expr`.
    fn loop_body(
        &mut self,
        expr: ExprId,
        pat: PatId,
        item: R,
        body: ExprId,
        exit: Label,
        cont: Label,
    ) {
        self.irrefutable(pat, item);
        let t = self.temp();
        self.loops.push(LoopFrame {
            expr,
            brk: exit,
            cont,
            dst: t,
        });
        self.expr(body, t);
        self.loops.pop();
    }

    fn try_expr(&mut self, inner: ExprId, dst: R) {
        let inner_ty = self.ty(inner);
        let std = self.em.std;
        let (adt, success) = match strip_refs(&inner_ty) {
            Ty::Adt { item, .. } if *item == std.result.item => {
                (std.result.item, std.result.variant("Ok").unwrap_or(0))
            }
            _ => (std.option.item, std.option.variant("Some").unwrap_or(1)),
        };
        let value = self.value(inner);
        let ty = self.em.type_id(adt);
        let c = self.temp();
        self.emit(Instr::IsVariant {
            dst: reg(c),
            src: reg(value),
            ty,
            variant: success,
        });
        let ok = self.label();
        self.emit(Instr::JmpIf {
            cond: reg(c),
            target: ok,
        });
        if matches!(inner_ty, Ty::Ref { .. }) {
            let loaded = self.temp();
            self.emit(Instr::Load {
                dst: reg(loaded),
                src: reg(value),
            });
            self.emit(Instr::Ret { src: reg(loaded) });
        } else {
            self.emit(Instr::Ret { src: reg(value) });
        }
        self.bind(ok);
        self.emit(Instr::Field {
            dst: reg(dst),
            src: reg(value),
            index: 0,
        });
    }

    fn unary(&mut self, expr: ExprId, op: UnaryOp, inner: ExprId, dst: R) {
        let value = self.operand(inner, &[]);
        if op == UnaryOp::Deref {
            self.emit(Instr::Load {
                dst: reg(dst),
                src: reg(value),
            });
            return;
        }
        let operand = self.ty(inner);
        let result = self.ty(expr);
        let instr = match (op, strip_refs(&operand)) {
            (UnaryOp::Not, Ty::Bool) => Instr::Not {
                dst: reg(dst),
                src: reg(value),
            },
            (UnaryOp::Not, _) => Instr::IntUn {
                op: IntUnOp::Not,
                dst: reg(dst),
                src: reg(value),
                ty: self.em.int_of(strip_refs(&result)),
            },
            (_, Ty::Adt { item, .. }) if Some(*item) == self.em.sigs.std.decimal => Instr::DecNeg {
                dst: reg(dst),
                src: reg(value),
            },
            (_, Ty::Adt { item, .. }) if Some(*item) == self.em.sigs.std.rat => Instr::RatNeg {
                dst: reg(dst),
                src: reg(value),
            },
            _ => Instr::IntUn {
                op: IntUnOp::Neg,
                dst: reg(dst),
                src: reg(value),
                ty: self.em.int_of(strip_refs(&result)),
            },
        };
        self.emit(instr);
    }

    /// The arithmetic or bit operator `op` on operands of type `operand`, with the result
    /// type `result` (whose range an integer result must fit).
    fn arith(&mut self, op: BinaryOp, operand: &Ty, result: &Ty, dst: R, lhs: R, rhs: R) {
        let (dst, lhs, rhs) = (reg(dst), reg(lhs), reg(rhs));
        let sigs = self.em.sigs;
        let std = &sigs.std;
        let instr = match strip_refs(operand) {
            Ty::Bool => match bool_op(op) {
                Some(op) => Instr::BoolBin { op, dst, lhs, rhs },
                None => Instr::Unreachable,
            },
            Ty::Adt { item, .. } if Some(*item) == std.decimal => match arith_op(op) {
                Some(op) => Instr::DecBin { op, dst, lhs, rhs },
                None => Instr::Unreachable,
            },
            Ty::Adt { item, .. } if Some(*item) == std.rat => match arith_op(op) {
                Some(op) => Instr::RatBin { op, dst, lhs, rhs },
                None => Instr::Unreachable,
            },
            _ => match int_op(op) {
                Some(op) => Instr::IntBin {
                    op,
                    dst,
                    lhs,
                    rhs,
                    ty: self.em.int_of(strip_refs(result)),
                },
                None => Instr::Unreachable,
            },
        };
        self.emit(instr);
    }

    fn binary(&mut self, expr: ExprId, op: BinaryOp, lhs: ExprId, rhs: ExprId, dst: R) {
        if matches!(op, BinaryOp::And | BinaryOp::Or) {
            let end = self.label();
            self.expr(lhs, dst);
            if op == BinaryOp::And {
                self.emit(Instr::JmpNot {
                    cond: reg(dst),
                    target: end,
                });
            } else {
                self.emit(Instr::JmpIf {
                    cond: reg(dst),
                    target: end,
                });
            }
            self.expr(rhs, dst);
            self.bind(end);
            return;
        }
        let left = self.operand(lhs, &[rhs]);
        let right = self.operand(rhs, &[]);
        if let Some(op) = cmp_op(op) {
            self.emit(Instr::Cmp {
                op,
                dst: reg(dst),
                lhs: reg(left),
                rhs: reg(right),
            });
            return;
        }
        let operand = self.ty(lhs);
        let result = self.ty(expr);
        self.arith(op, &operand, &result, dst, left, right);
    }

    fn assign(&mut self, op: Option<BinaryOp>, target: ExprId, value: ExprId) {
        let value = self.value(value);
        let bare = match (
            self.expr_data(target),
            self.b.and_then(|b| b.infer.paths.get(&target)),
        ) {
            (Some(Expr::Path(_)), Some(ValueRef::Local(local))) if !self.celled.contains(local) => {
                Some(*local)
            }
            _ => None,
        };
        let target_ty = self.ty(target);
        // Compound assignment: `Int<lo, hi>` computes in `i128` and saturates at the bounds
        // of the place.
        let compute =
            |this: &mut Self, op: BinaryOp, out: R, current: R| match strip_refs(&target_ty) {
                Ty::Bounded { .. } => {
                    this.arith(
                        op,
                        &target_ty,
                        &Ty::Int(crate::ty::IntTy::I128),
                        out,
                        current,
                        value,
                    );
                    let ty = this.em.int_of(strip_refs(&target_ty));
                    this.emit(Instr::IntUn {
                        op: IntUnOp::Clamp,
                        dst: reg(out),
                        src: reg(out),
                        ty,
                    });
                }
                _ => this.arith(op, &target_ty, &target_ty, out, current, value),
            };
        match (bare, op) {
            (Some(local), None) => {
                let r = self.local_reg(local);
                self.mov(r, value);
            }
            (Some(local), Some(op)) => {
                let r = self.local_reg(local);
                compute(self, op, r, r);
            }
            (None, None) => {
                let place = self.place(target);
                self.emit(Instr::Store {
                    dst: reg(place),
                    value: reg(value),
                });
            }
            (None, Some(op)) => {
                let place = self.place(target);
                let current = self.temp();
                self.emit(Instr::Load {
                    dst: reg(current),
                    src: reg(place),
                });
                compute(self, op, current, current);
                self.emit(Instr::Store {
                    dst: reg(place),
                    value: reg(current),
                });
            }
        }
    }

    fn record(&mut self, expr: ExprId, fields: &[(crate::hir::Name, ExprId)], dst: R) {
        let Some(ctor) = self.b.and_then(|b| b.infer.records.get(&expr)).cloned() else {
            self.emit(Instr::Unreachable);
            return;
        };
        let variant = ctor.variant.unwrap_or(0);
        let names: Vec<Option<String>> = self
            .em
            .sigs
            .adt(ctor.adt)
            .and_then(|adt| adt.variants.get(variant as usize))
            .map(|variant| {
                variant
                    .fields
                    .iter()
                    .map(|field| field.name.as_ref().map(ToString::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let first = self.temps(names.len());
        for (name, value) in fields {
            match names
                .iter()
                .position(|field| field.as_deref() == Some(name.as_str()))
            {
                Some(index) => self.expr(*value, Self::nth(first, index)),
                None => self.emit(Instr::Unreachable),
            }
        }
        let ty = self.em.type_id(ctor.adt);
        self.emit(Instr::Adt {
            dst: reg(dst),
            first: reg(first),
            count: count(names.len()),
            ty,
            variant,
        });
    }

    fn closure(&mut self, expr: ExprId, dst: R) {
        let captures = self
            .b
            .and_then(|b| b.infer.closures.get(&expr))
            .map(|info| info.captures.clone())
            .unwrap_or_default();
        let first = self.temps(captures.len());
        for (index, local) in captures.iter().enumerate() {
            self.read_local(*local, Self::nth(first, index));
        }
        let func = self.em.request(Instance::Closure {
            owner: Box::new(self.root.clone()),
            expr,
        });
        self.emit(Instr::Closure {
            dst: reg(dst),
            captures: reg(first),
            count: count(captures.len()),
            func,
        });
    }

    // ----- Places -------------------------------------------------------------------

    /// A register holding a reference to the place `expr` denotes; a new cell for an
    /// expression that is not a place.
    fn place(&mut self, expr: ExprId) -> R {
        let Some(b) = self.b else {
            return self.temp();
        };
        match b.body.store.expr(expr) {
            Expr::Path(_) => match b.infer.paths.get(&expr) {
                Some(ValueRef::Local(local))
                    if self.celled.contains(local) || is_ref(self.local_ty(*local)) =>
                {
                    return self.local_reg(*local);
                }
                Some(ValueRef::Static(item)) if self.is_static_mut(*item) => {
                    return self.static_place(*item);
                }
                _ => {}
            },
            Expr::Field { base, .. } => {
                let field = b.infer.fields.get(&expr).copied();
                let derefs = field.map_or(0, |field| field.derefs);
                let base = self.place_after(*base, derefs);
                let t = self.temp();
                self.emit(Instr::RefField {
                    dst: reg(t),
                    base: reg(base),
                    index: field
                        .and_then(|field| u16::try_from(field.index).ok())
                        .unwrap_or(0),
                });
                return t;
            }
            Expr::Index { base, index } => {
                let base_place = if is_ref(self.raw_ty(*base)) {
                    self.value(*base)
                } else {
                    self.place(*base)
                };
                let index = self.value(*index);
                let t = self.temp();
                self.emit(Instr::RefIndex {
                    dst: reg(t),
                    base: reg(base_place),
                    index: reg(index),
                });
                return t;
            }
            Expr::Unary {
                op: UnaryOp::Deref,
                expr: inner,
            } => return self.value(*inner),
            _ => {}
        }
        let value = self.value(expr);
        let t = self.temp();
        self.emit(Instr::Cell {
            dst: reg(t),
            init: reg(value),
        });
        t
    }

    /// The place of `expr` dereferenced `derefs` times: with a dereference, the value of
    /// `expr` is the reference (references to references collapse).
    fn place_after(&mut self, expr: ExprId, derefs: u32) -> R {
        if derefs > 0 {
            self.value(expr)
        } else {
            self.place(expr)
        }
    }

    // ----- Calls --------------------------------------------------------------------

    fn call_expr(&mut self, expr: ExprId, callee: ExprId, args: &[ExprId], dst: R) {
        let direct = match (
            self.expr_data(callee),
            self.b.and_then(|b| b.infer.paths.get(&callee)),
        ) {
            (Some(Expr::Path(_)), Some(ValueRef::Fn(fn_ref))) => {
                Some(subst_fn_ref(fn_ref, &self.env))
            }
            _ => None,
        };
        match direct {
            Some(fn_ref) => {
                let first = self.list(args);
                let ret_ty = self.ty(expr);
                self.call(&fn_ref, first, args.len(), dst, &ret_ty, Some(args));
            }
            None => {
                let callee_ty = self.ty(callee);
                let target = self.value(callee);
                let first = self.list(args);
                let kind = self.value_kind(&callee_ty);
                self.emit(Instr::CallVal {
                    dst: reg(dst),
                    args: reg(first),
                    argc: count(args.len()),
                    callee: reg(target),
                    kind,
                });
            }
        }
    }

    /// The kind a function value of type `ty` has.
    fn value_kind(&self, ty: &Ty) -> BcFnKind {
        match strip_refs(ty) {
            Ty::FnPtr { kind, .. } => match kind {
                FnPtrKind::Plain => BcFnKind::Fn,
                FnPtrKind::Const => BcFnKind::Const,
                FnPtrKind::Pushdown => BcFnKind::Instant,
                FnPtrKind::Scene => BcFnKind::Scene,
            },
            Ty::Closure { kind, .. } => match kind {
                ClosureKind::Plain => BcFnKind::Fn,
                ClosureKind::Const => BcFnKind::Const,
                ClosureKind::Pushdown => BcFnKind::Instant,
            },
            Ty::FnDef(fn_ref) => match fn_ref.def {
                FnDefId::Item(item) => self.em.fn_kind(item),
                _ => BcFnKind::Const,
            },
            _ => BcFnKind::Fn,
        }
    }

    fn method_call(&mut self, expr: ExprId, receiver: ExprId, args: &[ExprId], dst: R) {
        let Some(call) = self
            .b
            .and_then(|b| b.infer.method_calls.get(&expr))
            .cloned()
        else {
            self.emit(Instr::Unreachable);
            return;
        };
        let fn_ref = subst_fn_ref(&call.func, &self.env);
        let first = self.temps(args.len().saturating_add(1));
        if call.receiver.autoref.is_some() {
            let place = self.place_after(receiver, call.receiver.derefs);
            self.mov(first, place);
        } else {
            self.expr(receiver, first);
            if call.receiver.derefs > 0 {
                self.emit(Instr::Load {
                    dst: reg(first),
                    src: reg(first),
                });
            }
        }
        for (index, arg) in args.iter().enumerate() {
            self.expr(*arg, Self::nth(first, index.saturating_add(1)));
        }
        let ret_ty = self.ty(expr);
        self.call(
            &fn_ref,
            first,
            args.len().saturating_add(1),
            dst,
            &ret_ty,
            None,
        );
    }

    /// A direct call of `fn_ref` with the arguments in `first..first+n`.
    fn call(
        &mut self,
        fn_ref: &FnRef,
        first: R,
        n: usize,
        dst: R,
        ret_ty: &Ty,
        args: Option<&[ExprId]>,
    ) {
        match fn_ref.def {
            FnDefId::StructCtor(item) | FnDefId::VariantCtor(item, _) => {
                let variant = match fn_ref.def {
                    FnDefId::VariantCtor(_, variant) => variant,
                    _ => 0,
                };
                let ty = self.em.type_id(item);
                self.emit(Instr::Adt {
                    dst: reg(dst),
                    first: reg(first),
                    count: count(n),
                    ty,
                    variant,
                });
            }
            FnDefId::Item(item) => match self.em.std.builtin(item) {
                Some(builtin) => self.builtin(item, builtin, fn_ref, first, n, dst, ret_ty, args),
                None => {
                    let func = self.em.request(Instance::Fn(fn_ref.clone()));
                    self.emit(Instr::Call {
                        dst: reg(dst),
                        args: reg(first),
                        argc: count(n),
                        func,
                    });
                }
            },
        }
    }

    /// A builtin: an intrinsic instruction, or a call of the engine's import.
    #[allow(clippy::too_many_arguments)]
    fn builtin(
        &mut self,
        item: ItemId,
        builtin: Builtin,
        fn_ref: &FnRef,
        first: R,
        n: usize,
        dst: R,
        ret_ty: &Ty,
        args: Option<&[ExprId]>,
    ) {
        let std = self.em.std;
        let (d, a, b1, b2) = (
            reg(dst),
            reg(first),
            reg(Self::nth(first, 1)),
            reg(Self::nth(first, 2)),
        );
        if item == std.push_step {
            self.emit(Instr::Step { dst: d, record: a });
            return;
        }
        if item == std.rng_choose {
            let domain = match fn_ref.args.first() {
                Some(crate::ty::GenericArg::Type(Ty::Adt { item, .. })) => {
                    Some(RandDomain::Enum(self.em.type_id(*item)))
                }
                Some(crate::ty::GenericArg::Type(ty @ Ty::Bounded { .. })) => {
                    Some(RandDomain::Int(self.em.int_type(int_type_of(ty))))
                }
                _ => None,
            };
            if let Some(domain) = domain {
                self.emit(Instr::Rand { dst: d, domain });
                return;
            }
        }
        let instr = match builtin {
            Builtin::Asset => {
                let path = args
                    .and_then(|args| args.first())
                    .and_then(|arg| self.expr_data(*arg))
                    .and_then(|data| match data {
                        Expr::Literal(Literal::String(path)) => Some(path.clone()),
                        _ => None,
                    });
                let asset_item = match fn_ref.args.last() {
                    Some(crate::ty::GenericArg::Type(Ty::Adt { item, .. })) => Some(*item),
                    _ => match ret_ty {
                        Ty::Adt { item, .. } => Some(*item),
                        _ => None,
                    },
                };
                let kind = asset_item.and_then(|item| std.asset_kind(item));
                match (path, kind, self.b) {
                    (Some(path), Some(kind), Some(b)) => {
                        let file = b.item.file.path(self.em.db).parent_dir_join(&path);
                        let value = ConstValue::Asset { kind, path: file };
                        self.load_value(dst, &value);
                        return;
                    }
                    _ => self.import_call(item, first, n, dst),
                }
            }
            Builtin::VecNew => Instr::VecNew { dst: d },
            Builtin::VecLen => Instr::Len { dst: d, src: a },
            Builtin::VecIsEmpty => {
                let len = self.temp();
                let zero = self.temp();
                self.emit(Instr::Len {
                    dst: reg(len),
                    src: a,
                });
                self.emit(Instr::Int {
                    dst: reg(zero),
                    value: 0,
                });
                Instr::Cmp {
                    op: CmpOp::Eq,
                    dst: d,
                    lhs: reg(len),
                    rhs: reg(zero),
                }
            }
            Builtin::VecGet => Instr::VecGet {
                dst: d,
                vec: a,
                index: b1,
                option: self.em.type_id(std.option.item),
            },
            Builtin::VecSet => {
                self.emit(Instr::VecSet {
                    vec: a,
                    index: b1,
                    value: b2,
                });
                Instr::Tuple {
                    dst: d,
                    first: 0,
                    count: 0,
                }
            }
            Builtin::VecPush => {
                self.emit(Instr::VecPush { vec: a, value: b1 });
                Instr::Tuple {
                    dst: d,
                    first: 0,
                    count: 0,
                }
            }
            Builtin::IntegerInto => Instr::Move { dst: d, src: a },
            Builtin::IntegerSaturate => Instr::IntUn {
                op: IntUnOp::Clamp,
                dst: d,
                src: a,
                ty: self.em.int_of(strip_refs(ret_ty)),
            },
            Builtin::Ruby => Instr::Ruby {
                dst: d,
                base: a,
                reading: b1,
            },
            Builtin::MonoRuby => Instr::MonoRuby {
                dst: d,
                base: a,
                readings: b1,
            },
            Builtin::Effect => Instr::Effect {
                dst: d,
                base: a,
                effect: b1,
            },
            Builtin::Concat => Instr::Concat { dst: d, parts: a },
            Builtin::ToText => Instr::ToText { dst: d, src: a },
            Builtin::RatNew => Instr::RatNew {
                dst: d,
                num: a,
                den: b1,
            },
            Builtin::RatToDecimal => Instr::RatDec { dst: d, src: a },
            _ => self.import_call(item, first, n, dst),
        };
        self.emit(instr);
    }

    fn import_call(&mut self, item: ItemId, first: R, n: usize, dst: R) -> Instr {
        let import = self.em.import(item);
        Instr::CallImp {
            dst: reg(dst),
            args: reg(first),
            argc: count(n),
            import,
        }
    }

    // ----- Patterns -----------------------------------------------------------------

    /// Whether `pat` matches through a reference: its type is one, or an enclosing pattern
    /// already did.
    fn place_mode(&self, pat: PatId, inherited: bool) -> bool {
        inherited || is_ref(self.pat_ty(pat))
    }

    /// An irrefutable pattern (`let`, parameters, `for`): a mismatch is unreachable.
    fn irrefutable(&mut self, pat: PatId, src: R) {
        let fail = self.unreachable_label();
        let place = self.place_mode(pat, false);
        self.pat(pat, src, place, fail);
    }

    fn bind_local(&mut self, local: LocalId, src: R) {
        let r = self.local_reg(local);
        if self.celled.contains(&local) {
            self.emit(Instr::Cell {
                dst: reg(r),
                init: reg(src),
            });
        } else {
            self.mov(r, src);
        }
    }

    fn ctor_test(&mut self, ctor: &CtorRef, src: R, fail: Label) {
        let is_enum = self.em.sigs.adt(ctor.adt).is_some_and(|adt| adt.is_enum);
        if !is_enum {
            return;
        }
        let ty = self.em.type_id(ctor.adt);
        let c = self.temp();
        self.emit(Instr::IsVariant {
            dst: reg(c),
            src: reg(src),
            ty,
            variant: ctor.variant.unwrap_or(0),
        });
        self.emit(Instr::JmpNot {
            cond: reg(c),
            target: fail,
        });
    }

    /// Field `index` of `src` (a reference to it in place mode) into `dst`.
    fn project_into(&mut self, dst: R, src: R, index: usize, place: bool) {
        let index = u16::try_from(index).unwrap_or(u16::MAX);
        if place {
            self.emit(Instr::RefField {
                dst: reg(dst),
                base: reg(src),
                index,
            });
        } else {
            self.emit(Instr::Field {
                dst: reg(dst),
                src: reg(src),
                index,
            });
        }
    }

    /// Matches the sub-pattern `sub` against field `index` of `src`; a plain binding
    /// receives the field directly.
    fn sub_pat(&mut self, sub: PatId, src: R, index: usize, place: bool, fail: Label) {
        if let Some(local) = self.plain_binding(sub) {
            let r = self.local_reg(local);
            self.project_into(r, src, index, place);
            return;
        }
        let t = self.temp();
        self.project_into(t, src, index, place);
        self.pat(sub, t, place, fail);
    }

    /// Jumps to `fail` unless `src` equals `value`.
    fn test_eq(&mut self, src: R, value: &ConstValue, fail: Label) {
        let expected = self.temp();
        self.load_value(expected, value);
        let c = self.temp();
        self.emit(Instr::Cmp {
            op: CmpOp::Eq,
            dst: reg(c),
            lhs: reg(src),
            rhs: reg(expected),
        });
        self.emit(Instr::JmpNot {
            cond: reg(c),
            target: fail,
        });
    }

    fn bound_value(&mut self, pat: PatId, which: u8, bound: &PatBound) -> Option<ConstValue> {
        match bound {
            PatBound::Lit { lit, negated } => literal_value(lit, *negated),
            PatBound::Path(_) => {
                let Some(Resolution::Def(Def::Item(item))) =
                    self.b?.res.store.range_bounds.get(&(pat, which))
                else {
                    return None;
                };
                self.em.ctx.const_value(*item, Vec::new()).ok()
            }
        }
    }

    /// Matches `pat` against the value (or, in place mode, the reference) in `src`,
    /// binding its locals; jumps to `fail` on a mismatch.
    fn pat(&mut self, pat: PatId, src: R, place: bool, fail: Label) {
        let Some(b) = self.b else {
            return;
        };
        let place = self.place_mode(pat, place);
        let ctor = b.infer.pat_ctors.get(&pat).cloned();
        match b.body.store.pat(pat) {
            Pat::Missing | Pat::Wild | Pat::Rest => {}
            Pat::Bind { .. } => match ctor {
                Some(ctor) => self.ctor_test(&ctor, src, fail),
                None => {
                    if let Some(local) = b.res.bindings.get(&pat).copied() {
                        self.bind_local(local, src);
                    }
                }
            },
            Pat::Lit { lit, negated } => match literal_value(lit, *negated) {
                Some(value) => self.test_eq(src, &value, fail),
                None => self.emit(Instr::Unreachable),
            },
            Pat::Range {
                start,
                end,
                inclusive,
            } => {
                for (which, bound, op) in [
                    (0u8, start, CmpOp::Ge),
                    (1u8, end, if *inclusive { CmpOp::Le } else { CmpOp::Lt }),
                ] {
                    let Some(bound) = bound else {
                        continue;
                    };
                    let Some(value) = self.bound_value(pat, which, bound) else {
                        self.emit(Instr::Unreachable);
                        continue;
                    };
                    let k = self.temp();
                    self.load_value(k, &value);
                    let c = self.temp();
                    self.emit(Instr::Cmp {
                        op,
                        dst: reg(c),
                        lhs: reg(src),
                        rhs: reg(k),
                    });
                    self.emit(Instr::JmpNot {
                        cond: reg(c),
                        target: fail,
                    });
                }
            }
            Pat::Tuple(pats) => {
                let len = match self.pat_ty(pat).map(strip_refs) {
                    Some(Ty::Tuple(fields)) => fields.len(),
                    _ => pats.len(),
                };
                self.fields(pats, src, place, len, fail);
            }
            Pat::TupleStruct { pats, .. } => {
                let Some(ctor) = ctor else {
                    self.emit(Instr::Unreachable);
                    return;
                };
                self.ctor_test(&ctor, src, fail);
                let len = self
                    .em
                    .sigs
                    .adt(ctor.adt)
                    .and_then(|adt| adt.variants.get(ctor.variant.unwrap_or(0) as usize))
                    .map_or(pats.len(), |variant| variant.fields.len());
                self.fields(pats, src, place, len, fail);
            }
            Pat::Record { fields, .. } => {
                let Some(ctor) = ctor else {
                    self.emit(Instr::Unreachable);
                    return;
                };
                self.ctor_test(&ctor, src, fail);
                let names: Vec<Option<String>> = self
                    .em
                    .sigs
                    .adt(ctor.adt)
                    .and_then(|adt| adt.variants.get(ctor.variant.unwrap_or(0) as usize))
                    .map(|variant| {
                        variant
                            .fields
                            .iter()
                            .map(|field| field.name.as_ref().map(ToString::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                for (name, sub) in fields {
                    let Some(index) = names
                        .iter()
                        .position(|field| field.as_deref() == Some(name.as_str()))
                    else {
                        self.emit(Instr::Unreachable);
                        continue;
                    };
                    if matches!(b.body.store.pat(*sub), Pat::Wild | Pat::Rest) {
                        continue;
                    }
                    self.sub_pat(*sub, src, index, place, fail);
                }
            }
            Pat::Path(_) => match ctor {
                Some(ctor) => self.ctor_test(&ctor, src, fail),
                None => {
                    let value = match b.res.store.pats.get(&pat) {
                        Some(Resolution::Def(Def::Item(item))) => {
                            self.em.ctx.const_value(*item, Vec::new()).ok()
                        }
                        _ => None,
                    };
                    match value {
                        Some(value) => self.test_eq(src, &value, fail),
                        None => self.emit(Instr::Unreachable),
                    }
                }
            },
            Pat::Or(alternatives) => {
                let ok = self.label();
                let last = alternatives.len().saturating_sub(1);
                for (index, alternative) in alternatives.iter().enumerate() {
                    if index == last {
                        self.pat(*alternative, src, place, fail);
                    } else {
                        let next = self.label();
                        self.pat(*alternative, src, place, next);
                        self.jmp(ok);
                        self.bind(next);
                    }
                }
                self.bind(ok);
            }
        }
    }

    /// Matches `pats` (with at most one `..`) against the `len` fields of `src`.
    fn fields(&mut self, pats: &[PatId], src: R, place: bool, len: usize, fail: Label) {
        let Some(b) = self.b else {
            return;
        };
        let store = &b.body.store;
        let rest = pats
            .iter()
            .position(|pat| matches!(store.pat(*pat), Pat::Rest));
        let indexed: Vec<(usize, PatId)> = match rest {
            None => pats.iter().copied().enumerate().collect(),
            Some(rest) => {
                let after = pats.len().saturating_sub(rest).saturating_sub(1);
                let skip = len.saturating_sub(after);
                let mut out: Vec<(usize, PatId)> =
                    pats.iter().copied().take(rest).enumerate().collect();
                out.extend(
                    pats.iter()
                        .copied()
                        .skip(rest.saturating_add(1))
                        .enumerate()
                        .map(|(index, pat)| (skip.saturating_add(index), pat)),
                );
                out
            }
        };
        for (index, sub) in indexed {
            if matches!(store.pat(sub), Pat::Wild | Pat::Rest) {
                continue;
            }
            self.sub_pat(sub, src, index, place, fail);
        }
    }
}
