//! Per-item summaries of navigation operations, calls and bindings, built from MIR.

use rustc_hash::FxHashSet;

use crate::db::Db;
use crate::def_map::def_maps;
use crate::diagnostic::{Anchor, NodeId, Owner};
use crate::hir::{Expr, ExprId, ExprStore, body_of};
use crate::infer::walk::expr_children;
use crate::infer::{InferenceResult, inference};
use crate::input::Program;
use crate::item_tree::{FnKind, ItemId, ItemKind, item_tree};
use crate::mir::{
    AggregateKind, Body, Callee, ConstOperand, MirSource, Operand, Rvalue, StatementKind,
    Terminator, item_mir,
};
use crate::signatures::program_signatures;
use crate::ty::{FnDefId, FnRef};

use super::code::{BindingAction, SystemStd, fn_target};
use super::{ModeKind, Op, OpKind};

/// What one part of a body (its own code or the exit handlers it passes) does directly.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Region {
    pub(super) ops: Vec<Op>,
    /// Items called, and constants and statics read.
    pub(super) calls: Vec<ItemId>,
    /// Functions referred to as values (handlers, pointers passed on).
    pub(super) values: Vec<ItemId>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Summary {
    pub(super) own: Region,
    /// Code inside the exit handlers of `nav::start`: it runs in the game mode.
    pub(super) exit: Region,
    pub(super) bindings: Vec<BindingAction>,
}

/// The std items the system checks recognize.
#[salsa::tracked(returns(ref))]
pub(super) fn system_std(db: &dyn Db, program: Program) -> Option<SystemStd> {
    SystemStd::new(db, def_maps(db, program), program_signatures(db, program))
}

/// The summary of an item's body: non-pushdown fns, consts and statics (a pushdown fn
/// cannot call a `fn`, so it never runs system code).
#[salsa::tracked(returns(ref))]
pub(super) fn system_summary(db: &dyn Db, program: Program, item: ItemId) -> Summary {
    let follows = match item_tree(db, item.file)
        .item(item.index)
        .map(|data| &data.kind)
    {
        Some(ItemKind::Fn { kind, has_body, .. }) => *has_body && *kind != FnKind::Pushdown,
        Some(ItemKind::Const { has_value }) => *has_value,
        Some(ItemKind::Static { .. }) => true,
        _ => false,
    };
    if !follows {
        return Summary::default();
    }
    let (Some(std), Some(mir), Some(hir), Some(infer)) = (
        system_std(db, program).as_ref(),
        item_mir(db, program, item).as_ref(),
        body_of(db, item),
        inference(db, program, item),
    ) else {
        return Summary::default();
    };
    let mut builder = Builder {
        std,
        store: &hir.store,
        infer,
        item,
        root: hir.value,
        exit: FxHashSet::default(),
        skip: FxHashSet::default(),
        claimed: FxHashSet::default(),
        summary: Summary::default(),
    };
    for body in mir.bodies() {
        builder.mark(body);
    }
    for (body, src) in std::iter::once((&mir.main, None))
        .chain(mir.closures.iter().map(|(expr, body)| (body, Some(*expr))))
        .chain(
            mir.promoted
                .iter()
                .zip(mir.promoted_src.iter().map(|src| Some(*src))),
        )
    {
        builder.body(body, src);
    }
    let mut summary = builder.summary;
    for region in [&mut summary.own, &mut summary.exit] {
        region.ops.sort_by_key(|op| op.order);
    }
    summary
}

struct Builder<'a> {
    std: &'a SystemStd,
    store: &'a ExprStore,
    infer: &'a InferenceResult,
    item: ItemId,
    /// The root expression of the item's HIR body.
    root: ExprId,
    /// The HIR expressions inside the exit handlers of `nav::start`.
    exit: FxHashSet<ExprId>,
    /// The HIR expressions of operation arguments naming a target mode: edges of the mode
    /// graph, not code of the mode.
    skip: FxHashSet<ExprId>,
    /// The path expressions already anchoring an operation used as a value.
    claimed: FxHashSet<ExprId>,
    summary: Summary,
}

/// The arguments of a call expression, without the receiver of a method call.
fn hir_args(store: &ExprStore, expr: ExprId) -> &[ExprId] {
    match store.expr(expr) {
        Expr::Call { args, .. } | Expr::MethodCall { args, .. } => args,
        _ => &[],
    }
}

/// The item a callee names, for direct calls of items.
fn callee_ref(callee: &Callee) -> Option<&FnRef> {
    match callee {
        Callee::Fn(fn_ref) | Callee::Builtin(_, fn_ref) => Some(fn_ref),
        Callee::Value(_) => None,
    }
}

impl Builder<'_> {
    fn op_of(&self, fn_ref: &FnRef) -> Option<OpKind> {
        match fn_ref.def {
            FnDefId::Item(item) => self.std.ops.get(&item).copied(),
            _ => None,
        }
    }

    /// The target of an operation call at `src` and the MIR index of its argument.
    fn op_target(&self, op: OpKind, src: ExprId) -> (Option<ItemId>, usize, Option<ExprId>) {
        // `Narrative::call(&game, o)` names the receiver first; the MIR arguments of a
        // method call start with the receiver too.
        let mir_index = usize::from(op == OpKind::GameCall);
        let hir_index = match self.store.expr(src) {
            Expr::Call { .. } => mir_index,
            _ => 0,
        };
        let arg = hir_args(self.store, src).get(hir_index).copied();
        let target = match op.target_kind() {
            Some(ModeKind::Game) | None => None,
            Some(_) => arg.and_then(|arg| fn_target(self.store, self.infer, arg)),
        };
        (target, mir_index, arg)
    }

    fn subtree(&self, root: ExprId, out: &mut FxHashSet<ExprId>) {
        let mut stack = vec![root];
        while let Some(expr) = stack.pop() {
            if out.insert(expr) {
                expr_children(
                    self.store.expr(expr),
                    &mut |child| stack.push(child),
                    &mut |_| {},
                );
            }
        }
    }

    /// Collects the exit handler and target subtrees of the operation calls in `body`.
    fn mark(&mut self, body: &Body) {
        for (_, block) in body.blocks.iter() {
            for stmt in &block.stmts {
                let (StatementKind::Call { callee, .. }, Some(src)) = (&stmt.kind, stmt.src) else {
                    continue;
                };
                let Some(op) = callee_ref(callee).and_then(|fn_ref| self.op_of(fn_ref)) else {
                    continue;
                };
                let (target, _, arg) = self.op_target(op, src);
                if let (Some(_), Some(arg)) = (target, arg) {
                    let mut skip = std::mem::take(&mut self.skip);
                    self.subtree(arg, &mut skip);
                    self.skip = skip;
                }
                // `nav::start(entry, args, on_exit)`: the exit handler runs in the game mode.
                if op == OpKind::Start
                    && let Some(&handler) = hir_args(self.store, src).get(2)
                {
                    let mut exit = std::mem::take(&mut self.exit);
                    self.subtree(handler, &mut exit);
                    self.exit = exit;
                }
            }
        }
    }

    fn region(&mut self, exit: bool) -> &mut Region {
        if exit {
            &mut self.summary.exit
        } else {
            &mut self.summary.own
        }
    }

    fn record_op(&mut self, kind: OpKind, target: Option<ItemId>, expr: ExprId, exit: bool) {
        let op = Op {
            kind,
            target,
            anchor: Anchor::Node(Owner::Body(self.item), NodeId::Expr(expr)),
            order: (self.item.index, self.store.source_order(expr)),
        };
        self.region(exit).ops.push(op);
    }

    /// The `Action` variant a constructor names: `Open` or `OpenInGame`.
    fn action_ctor(&self, fn_ref: &FnRef) -> Option<fn(Option<ItemId>) -> BindingAction> {
        let action = self.std.action.as_ref()?;
        let FnDefId::VariantCtor(adt, variant) = fn_ref.def else {
            return None;
        };
        if adt != action.item {
            None
        } else if Some(variant) == action.open {
            Some(BindingAction::Open)
        } else if Some(variant) == action.open_in_game {
            Some(BindingAction::OpenInGame)
        } else {
            None
        }
    }

    /// The first unclaimed path expression under `src`, in source order, naming `item`.
    fn claim_path(&mut self, src: ExprId, item: ItemId) -> ExprId {
        let mut stack = vec![src];
        while let Some(expr) = stack.pop() {
            if matches!(self.store.expr(expr), Expr::Path(_))
                && !self.claimed.contains(&expr)
                && fn_target(self.store, self.infer, expr) == Some(item)
            {
                self.claimed.insert(expr);
                return expr;
            }
            let start = stack.len();
            expr_children(
                self.store.expr(expr),
                &mut |child| stack.push(child),
                &mut |_| {},
            );
            stack[start..].reverse();
        }
        src
    }

    fn body(&mut self, body: &Body, src: Option<ExprId>) {
        let body_exit = src.is_some_and(|src| self.exit.contains(&src));
        let fallback = match body.source {
            MirSource::Closure { expr, .. } => expr,
            _ => src.unwrap_or(self.root),
        };
        for (_, block) in body.blocks.iter() {
            for stmt in &block.stmts {
                let at = stmt.src.unwrap_or(fallback);
                if self.skip.contains(&at) {
                    continue;
                }
                let exit = stmt.src.map_or(body_exit, |src| self.exit.contains(&src));
                match &stmt.kind {
                    StatementKind::Assign(_, rvalue) => self.rvalue(rvalue, at, exit),
                    StatementKind::Call { callee, args, .. } => self.call(callee, args, at, exit),
                }
            }
            let at = block.term_src.unwrap_or(fallback);
            if self.skip.contains(&at) {
                continue;
            }
            let exit = block
                .term_src
                .map_or(body_exit, |src| self.exit.contains(&src));
            match &block.term {
                Terminator::SwitchBool { cond, .. } => self.operand(cond, at, exit),
                Terminator::Step { arg, .. } => self.operand(arg, at, exit),
                Terminator::SceneCall { callee, args, .. } => self.call(callee, args, at, exit),
                Terminator::Goto(_)
                | Terminator::SwitchVariant { .. }
                | Terminator::Return
                | Terminator::Unreachable
                | Terminator::Rng { .. } => {}
            }
        }
    }

    fn call(&mut self, callee: &Callee, args: &[Operand], at: ExprId, exit: bool) {
        let Some(fn_ref) = callee_ref(callee) else {
            if let Callee::Value(value) = callee {
                self.operand(value, at, exit);
            }
            for arg in args {
                self.operand(arg, at, exit);
            }
            return;
        };
        if let Some(op) = self.op_of(fn_ref) {
            let (target, target_index, _) = self.op_target(op, at);
            self.record_op(op, target, at, exit);
            for (index, arg) in args.iter().enumerate() {
                if index == target_index && target.is_some() {
                    continue;
                }
                let arg_exit = exit || (op == OpKind::Start && index == 2);
                self.operand(arg, at, arg_exit);
            }
            return;
        }
        match fn_ref.def {
            FnDefId::Item(item) => self.region(exit).calls.push(item),
            FnDefId::VariantCtor(..) => {
                if let Some(ctor) = self.action_ctor(fn_ref) {
                    let target = hir_args(self.store, at)
                        .first()
                        .and_then(|&arg| fn_target(self.store, self.infer, arg));
                    self.summary.bindings.push(ctor(target));
                }
            }
            FnDefId::StructCtor(_) => {}
        }
        for arg in args {
            self.operand(arg, at, exit);
        }
    }

    fn rvalue(&mut self, rvalue: &Rvalue, at: ExprId, exit: bool) {
        match rvalue {
            Rvalue::Use(operand)
            | Rvalue::IntNeg { operand, .. }
            | Rvalue::IntNot { operand, .. }
            | Rvalue::IntClamp { operand, .. }
            | Rvalue::BoolNot(operand)
            | Rvalue::DecNeg(operand)
            | Rvalue::RatNeg(operand)
            | Rvalue::Repeat { operand, .. }
            | Rvalue::Cast { operand, .. } => self.operand(operand, at, exit),
            Rvalue::IntBin { lhs, rhs, .. }
            | Rvalue::BoolBin { lhs, rhs, .. }
            | Rvalue::DecBin { lhs, rhs, .. }
            | Rvalue::RatBin { lhs, rhs, .. }
            | Rvalue::Cmp { lhs, rhs, .. } => {
                self.operand(lhs, at, exit);
                self.operand(rhs, at, exit);
            }
            Rvalue::Aggregate { kind, ops } => {
                if let (AggregateKind::Adt(ctor), Some(action)) = (kind, &self.std.action)
                    && ctor.adt == action.item
                    && ctor.variant.is_some()
                    && ctor.variant == action.back
                {
                    self.summary.bindings.push(BindingAction::Back);
                }
                for operand in ops {
                    self.operand(operand, at, exit);
                }
            }
            Rvalue::Closure { captures, .. } => {
                for operand in captures {
                    self.operand(operand, at, exit);
                }
            }
            Rvalue::Ref { .. } | Rvalue::Len(_) => {}
        }
    }

    fn operand(&mut self, operand: &Operand, at: ExprId, exit: bool) {
        let Operand::Const(constant) = operand else {
            return;
        };
        match constant {
            ConstOperand::Fn(fn_ref) => match fn_ref.def {
                FnDefId::Item(item) => {
                    if let Some(&op) = self.std.ops.get(&item) {
                        // An operation used as a value (`on_click = nav::back`): it may run
                        // with any argument.
                        let path = self.claim_path(at, item);
                        self.record_op(op, None, path, exit);
                    } else {
                        self.region(exit).values.push(item);
                    }
                }
                FnDefId::VariantCtor(..) => {
                    if let Some(ctor) = self.action_ctor(fn_ref) {
                        self.summary.bindings.push(ctor(None));
                    }
                }
                FnDefId::StructCtor(_) => {}
            },
            ConstOperand::Item { item, .. }
            | ConstOperand::Handle(item)
            | ConstOperand::Static(item) => self.region(exit).calls.push(*item),
            ConstOperand::Value(_) | ConstOperand::Param { .. } | ConstOperand::Promoted(_) => {}
        }
    }
}
