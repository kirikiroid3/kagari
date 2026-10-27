//! The recursion check of instant procedures (`kinds.md`): a per-item scan of the MIR for
//! the edges of the call graph, and a program-wide cycle check over them.

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::{SmallVec, smallvec};

use crate::callee::{self, CalleeKind, classify};
use crate::db::Db;
use crate::def_map::def_maps;
use crate::diagnostic::{Anchor, Diag, DiagnosticCode, NodeId, Owner};
use crate::eval::{ConstExprs, const_exprs};
use crate::hir::{Expr, ExprId, ExprStore, body_of};
use crate::infer::{InferenceResult, ValueRef, inference};
use crate::input::Program;
use crate::item_tree::{ItemId, item_tree};
use crate::mir::{
    Body, BodyKind, Callee, ConstOperand, Operand, Rvalue, StatementKind, Terminator, item_mir,
};
use crate::ty::FnDefId;

use super::Env;
use super::body::{callee_of_ty, children, expr_ty, value_ty};

/// What the program-wide recursion check of instant procedures needs from one item: the
/// item's MIR bodies (`main`, closures and promoted constants), attributed to the item.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InstantEdges {
    /// Edges from an instant procedure: direct calls of instant procedures in its pushdown
    /// code and instant procedures used as values (which the receiver may call).
    pub calls: Vec<InstantCall>,
    /// Calls through a `pushdown fn` pointer or a `pushdown` closure value in the pushdown
    /// code of an instant procedure: the caller and the call.
    pub indirect: Vec<(ItemId, Anchor)>,
    /// Instant procedures used as values anywhere, and whether the value may escape: be
    /// stored, returned or folded into a constant, rather than passed straight to a
    /// function of the story as an argument.
    pub values: Vec<(ItemId, bool)>,
    /// Some code stores a runtime `pushdown fn` pointer (not a constant) into a place, so
    /// any instant procedure passed around may come back out of storage.
    pub stores_pointers: bool,
}

/// An edge of the call graph of instant procedures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InstantCall {
    pub caller: ItemId,
    pub callee: ItemId,
    pub anchor: Anchor,
    pub kind: EdgeKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EdgeKind {
    /// `callee(…)` or `x.callee(…)`.
    Direct,
    /// `callee` used as a value (a pointer or a closure) in the caller's body.
    Value,
    /// A call through a pointer or closure value that may hold `callee`.
    Indirect,
}

/// The edges of `item`; empty when it has no MIR (no body, or resolution or inference
/// errors).
#[salsa::tracked(returns(ref))]
pub fn instant_edges(db: &dyn Db, program: Program, item: ItemId) -> InstantEdges {
    let mut out = InstantEdges::default();
    let (Some(mir), Some(body), Some(infer)) = (
        item_mir(db, program, item).as_ref(),
        body_of(db, item),
        inference(db, program, item),
    ) else {
        return out;
    };
    let env = Env::new(db, program);
    let mut scan = Scan {
        env: &env,
        item,
        instant: mir.main.kind == BodyKind::Instant,
        store: &body.store,
        infer,
        consts: const_exprs(db, program, item),
        out: &mut out,
    };
    for body in mir.bodies() {
        scan.body(body);
    }
    out
}

/// Cycles of calls among instant procedures, each reported once at a call inside it.
///
/// Calls through pointers and closures count too, since an instant procedure must finish
/// without input. A value reaches a call either down the stack, as an argument, from the
/// code that names the function: then naming it is an edge from that code (`h(k)` inside
/// `k` is the edge `k → k`); or out of storage (a story variable, a constant, a returned
/// value): then every call through a value may call it. Functions named only as direct
/// arguments of story calls cannot come back out of storage unless some code stores a
/// runtime pointer, in which case every instant procedure used as a value may.
#[salsa::tracked(returns(ref))]
pub fn instant_recursion(db: &dyn Db, program: Program) -> Vec<Diag> {
    let maps = def_maps(db, program);
    let mut calls: Vec<InstantCall> = Vec::new();
    let mut indirect: Vec<(ItemId, Anchor)> = Vec::new();
    let mut escaping: Vec<ItemId> = Vec::new();
    let mut all_values: Vec<ItemId> = Vec::new();
    let mut stores_pointers = false;
    for &(file, _) in &maps.files {
        if !maps.file_modules.contains_key(&file) {
            continue;
        }
        for index in item_tree(db, file).indices() {
            let edges = instant_edges(db, program, ItemId { file, index });
            calls.extend(edges.calls.iter().copied());
            indirect.extend(edges.indirect.iter().copied());
            for &(item, escapes) in &edges.values {
                if !all_values.contains(&item) {
                    all_values.push(item);
                }
                if escapes && !escaping.contains(&item) {
                    escaping.push(item);
                }
            }
            stores_pointers |= edges.stores_pointers;
        }
    }
    let stored = if stores_pointers {
        &all_values
    } else {
        &escaping
    };
    for &(caller, anchor) in &indirect {
        calls.extend(stored.iter().map(|&callee| InstantCall {
            caller,
            callee,
            anchor,
            kind: EdgeKind::Indirect,
        }));
    }
    let mut graph: FxHashMap<ItemId, Vec<ItemId>> = FxHashMap::default();
    let mut kinds: FxHashMap<(ItemId, ItemId), EdgeKind> = FxHashMap::default();
    for call in &calls {
        let callees = graph.entry(call.caller).or_default();
        if !callees.contains(&call.callee) {
            callees.push(call.callee);
        }
        let kind = kinds.entry((call.caller, call.callee)).or_insert(call.kind);
        *kind = (*kind).min(call.kind);
    }
    let components = strongly_connected(&calls, &graph);
    let env = Env::new(db, program);
    let mut reported: FxHashSet<usize> = FxHashSet::default();
    let mut out = Vec::new();
    // Report at a direct call where the cycle has one.
    let mut ordered: Vec<&InstantCall> = calls.iter().collect();
    ordered.sort_by_key(|call| call.kind);
    for call in ordered {
        let (Some(&from), Some(&to)) = (components.get(&call.caller), components.get(&call.callee))
        else {
            continue;
        };
        if from != to || !reported.insert(from) {
            continue;
        }
        // The cycle: the caller, then the shortest way back from the callee.
        let mut path = vec![call.caller];
        path.extend(shortest_path(
            &graph,
            call.callee,
            call.caller,
            &components,
            from,
        ));
        let mut cycle = format!("`{}`", env.fn_path(call.caller));
        for pair in path.windows(2) {
            let [a, b] = pair else {
                continue;
            };
            let how = match kinds.get(&(*a, *b)) {
                Some(EdgeKind::Value) => " (passed as a value to code that may call it)",
                Some(EdgeKind::Indirect) => " (called through a pointer or closure)",
                _ => "",
            };
            cycle.push_str(&format!(" → `{}`{how}", env.fn_path(*b)));
        }
        out.push(Diag::new(
            call.anchor,
            DiagnosticCode::KindInstantRecursion,
            format!(
                "instant procedures cannot recurse, but these calls form a cycle: {cycle}; \
                 mark one of them `#[scene]` and pass through a step"
            ),
        ));
    }
    out
}

/// The scan of one item's MIR bodies.
struct Scan<'a, 'db> {
    env: &'a Env<'db>,
    item: ItemId,
    /// The item is an instant procedure: its edges are recorded.
    instant: bool,
    /// The item's HIR store, which the `src` of every body of the item points into.
    store: &'a ExprStore,
    infer: &'a InferenceResult,
    consts: Option<&'a ConstExprs>,
    out: &'a mut InstantEdges,
}

impl Scan<'_, '_> {
    fn body(&mut self, body: &Body) {
        // Pushdown code of an instant procedure: its main body or a `pushdown` closure.
        let pushdown = self.instant && body.kind == BodyKind::Instant;
        for (_, data) in body.blocks.iter() {
            for stmt in &data.stmts {
                match &stmt.kind {
                    StatementKind::Assign(_, rvalue) => {
                        self.note_store(stmt.src);
                        for operand in rvalue_operands(rvalue) {
                            self.value(operand, true, stmt.src, None);
                        }
                    }
                    StatementKind::Call { callee, args, .. } => {
                        self.call(callee, args, stmt.src, pushdown);
                    }
                }
            }
            match &data.term {
                Terminator::SwitchBool { cond, .. } => self.value(cond, true, data.term_src, None),
                // `push_step` receives its argument as a parameter of story code.
                Terminator::Step { arg, .. } => self.value(arg, false, data.term_src, Some(0)),
                Terminator::SceneCall { callee, args, .. } => {
                    self.call(callee, args, data.term_src, pushdown);
                }
                Terminator::Goto(_)
                | Terminator::SwitchVariant { .. }
                | Terminator::Return
                | Terminator::Unreachable
                | Terminator::Rng { .. } => {}
            }
        }
    }

    /// A call: values passed to it, then the edge it is.
    fn call(&mut self, callee: &Callee, args: &[Operand], src: Option<ExprId>, pushdown: bool) {
        let target = match callee {
            Callee::Fn(fn_ref) | Callee::Builtin(_, fn_ref) => match fn_ref.def {
                FnDefId::Item(item) => Some(callee::Callee::Item(item)),
                FnDefId::StructCtor(_) | FnDefId::VariantCtor(..) => None,
            },
            Callee::Value(_) => self.value_callee(src),
        };
        let kind = target
            .as_ref()
            .and_then(|target| classify(self.env.sigs, target));
        // Arguments of story code are received as parameters; anything else (a `const fn`
        // or a constructor) may fold them into a constant.
        let escapes = !matches!(
            kind,
            Some(CalleeKind::Instant | CalleeKind::Scene | CalleeKind::Step | CalleeKind::Rng)
        );
        for (index, arg) in args.iter().enumerate() {
            self.value(arg, escapes, src, Some(index));
        }
        if !pushdown || kind != Some(CalleeKind::Instant) {
            return;
        }
        match (callee, target) {
            (Callee::Fn(_) | Callee::Builtin(..), Some(callee::Callee::Item(callee))) => {
                self.out.calls.push(InstantCall {
                    caller: self.item,
                    callee,
                    anchor: self.anchor(src),
                    kind: EdgeKind::Direct,
                });
            }
            (Callee::Value(_), _) => {
                let anchor = self.anchor(src);
                self.out.indirect.push((self.item, anchor));
            }
            _ => {}
        }
    }

    /// What a call through a value calls, from the type of the HIR callee expression.
    fn value_callee(&self, src: Option<ExprId>) -> Option<callee::Callee> {
        let Expr::Call { callee, .. } = self.store.expr(src?) else {
            return None;
        };
        expr_ty(self.infer, *callee).and_then(callee_of_ty)
    }

    /// An operand that may name an instant procedure as a value.
    fn value(&mut self, operand: &Operand, escapes: bool, src: Option<ExprId>, arg: Option<usize>) {
        let Operand::Const(ConstOperand::Fn(fn_ref)) = operand else {
            return;
        };
        let FnDefId::Item(item) = fn_ref.def else {
            return;
        };
        if classify(self.env.sigs, &callee::Callee::Item(item)) != Some(CalleeKind::Instant) {
            return;
        }
        if !self.out.values.contains(&(item, escapes)) {
            self.out.values.push((item, escapes));
        }
        if self.instant {
            let at = src.map(|src| self.value_expr(src, item, arg));
            self.out.calls.push(InstantCall {
                caller: self.item,
                callee: item,
                anchor: self.anchor(at),
                kind: EdgeKind::Value,
            });
        }
    }

    /// The path naming `item` that an operand of the statement lowered from `src` comes
    /// from: the argument itself, else the first such path inside `src`, else `src`.
    fn value_expr(&self, src: ExprId, item: ItemId, arg: Option<usize>) -> ExprId {
        let direct = arg.and_then(|index| match self.store.expr(src) {
            Expr::Call { args, .. } => args.get(index).copied(),
            Expr::MethodCall { receiver, args, .. } => match index.checked_sub(1) {
                None => Some(*receiver),
                Some(index) => args.get(index).copied(),
            },
            _ => None,
        });
        if let Some(expr) = direct.filter(|&expr| self.names(expr, item)) {
            return expr;
        }
        let mut stack = vec![src];
        while let Some(expr) = stack.pop() {
            if self.names(expr, item) {
                return expr;
            }
            let data = self.store.expr(expr);
            match data {
                // A closure body is a body of its own; a callee path is no value.
                Expr::Closure { .. } => {}
                Expr::Call { callee, args } => {
                    stack.extend(args.iter().rev().copied());
                    if !matches!(self.store.expr(*callee), Expr::Path(_)) {
                        stack.push(*callee);
                    }
                }
                _ => stack.extend(children(data).into_iter().rev()),
            }
        }
        src
    }

    /// `expr` is a path to the function `item`.
    fn names(&self, expr: ExprId, item: ItemId) -> bool {
        matches!(self.store.expr(expr), Expr::Path(_))
            && matches!(
                self.infer.paths.get(&expr),
                Some(ValueRef::Fn(fn_ref)) if fn_ref.def == FnDefId::Item(item)
            )
    }

    /// An assignment `target = value` that stores a runtime `pushdown fn` pointer (not a
    /// constant): any instant procedure passed around may then come back out of storage.
    fn note_store(&mut self, src: Option<ExprId>) {
        let Some(src) = src else {
            return;
        };
        let Expr::Assign { value, .. } = self.store.expr(src) else {
            return;
        };
        if !self.consts.is_some_and(|consts| consts.contains(*value))
            && value_ty(self.infer, *value).is_some_and(|ty| self.env.contains_instant_pointer(ty))
        {
            self.out.stores_pointers = true;
        }
    }

    fn anchor(&self, expr: Option<ExprId>) -> Anchor {
        match expr {
            Some(expr) => Anchor::Node(Owner::Body(self.item), NodeId::Expr(expr)),
            None => Anchor::ItemName(self.item),
        }
    }
}

/// The operands an rvalue reads.
fn rvalue_operands(rvalue: &Rvalue) -> SmallVec<[&Operand; 4]> {
    match rvalue {
        Rvalue::Use(operand)
        | Rvalue::IntNeg { operand, .. }
        | Rvalue::IntNot { operand, .. }
        | Rvalue::IntClamp { operand, .. }
        | Rvalue::BoolNot(operand)
        | Rvalue::DecNeg(operand)
        | Rvalue::RatNeg(operand)
        | Rvalue::Repeat { operand, .. }
        | Rvalue::Cast { operand, .. } => smallvec![operand],
        Rvalue::IntBin { lhs, rhs, .. }
        | Rvalue::BoolBin { lhs, rhs, .. }
        | Rvalue::DecBin { lhs, rhs, .. }
        | Rvalue::RatBin { lhs, rhs, .. }
        | Rvalue::Cmp { lhs, rhs, .. } => smallvec![lhs, rhs],
        Rvalue::Aggregate { ops, .. } | Rvalue::Closure { captures: ops, .. } => {
            ops.iter().collect()
        }
        Rvalue::Ref { .. } | Rvalue::Len(_) => SmallVec::new(),
    }
}

/// The strongly connected component of every node that lies on a cycle (a component of
/// several nodes, or one node calling itself), numbered from 0.
fn strongly_connected(
    calls: &[InstantCall],
    graph: &FxHashMap<ItemId, Vec<ItemId>>,
) -> FxHashMap<ItemId, usize> {
    // Kosaraju with explicit stacks: finishing order on the graph, then components on the
    // reversed graph in reverse finishing order.
    let mut nodes: Vec<ItemId> = Vec::new();
    let mut seen_nodes: FxHashSet<ItemId> = FxHashSet::default();
    for call in calls {
        for node in [call.caller, call.callee] {
            if seen_nodes.insert(node) {
                nodes.push(node);
            }
        }
    }
    let mut reverse: FxHashMap<ItemId, Vec<ItemId>> = FxHashMap::default();
    for (from, tos) in graph {
        for to in tos {
            reverse.entry(*to).or_default().push(*from);
        }
    }
    let mut visited: FxHashSet<ItemId> = FxHashSet::default();
    let mut order: Vec<ItemId> = Vec::new();
    for &start in &nodes {
        if !visited.insert(start) {
            continue;
        }
        let mut stack: Vec<(ItemId, usize)> = vec![(start, 0)];
        while let Some((node, next)) = stack.pop() {
            let successors = graph.get(&node).map_or(&[][..], Vec::as_slice);
            if let Some(&succ) = successors.get(next) {
                stack.push((node, next.saturating_add(1)));
                if visited.insert(succ) {
                    stack.push((succ, 0));
                }
            } else {
                order.push(node);
            }
        }
    }
    let mut assigned: FxHashSet<ItemId> = FxHashSet::default();
    let mut component: FxHashMap<ItemId, usize> = FxHashMap::default();
    let mut count = 0usize;
    for &start in order.iter().rev() {
        if !assigned.insert(start) {
            continue;
        }
        let mut members = Vec::new();
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            members.push(node);
            for &pred in reverse.get(&node).map_or(&[][..], Vec::as_slice) {
                if assigned.insert(pred) {
                    stack.push(pred);
                }
            }
        }
        let cyclic = members.len() > 1
            || graph
                .get(&start)
                .is_some_and(|succs| succs.contains(&start));
        if cyclic {
            for member in members {
                component.insert(member, count);
            }
            count = count.saturating_add(1);
        }
    }
    component
}

/// The nodes of a shortest path from `from` to `to` inside component `id`, both included.
fn shortest_path(
    graph: &FxHashMap<ItemId, Vec<ItemId>>,
    from: ItemId,
    to: ItemId,
    components: &FxHashMap<ItemId, usize>,
    id: usize,
) -> Vec<ItemId> {
    let mut parent: FxHashMap<ItemId, ItemId> = FxHashMap::default();
    let mut queue = std::collections::VecDeque::from([from]);
    let mut seen: FxHashSet<ItemId> = FxHashSet::from_iter([from]);
    while let Some(node) = queue.pop_front() {
        if node == to {
            break;
        }
        for &succ in graph.get(&node).map_or(&[][..], Vec::as_slice) {
            if components.get(&succ) == Some(&id) && seen.insert(succ) {
                parent.insert(succ, node);
                queue.push_back(succ);
            }
        }
    }
    let mut path = vec![to];
    let mut node = to;
    while node != from {
        let Some(&prev) = parent.get(&node) else {
            break;
        };
        path.push(prev);
        node = prev;
    }
    path.reverse();
    path
}
