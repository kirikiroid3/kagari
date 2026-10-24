//! Story variables that never influence the story (`docs/syntax/src/compilation.md`, stage 5).
//!
//! A flow-insensitive dependence graph over every `pushdown fn` body of the program (game and
//! std), closures included. Its nodes are the story variables, the locals of each body, the
//! parameters and the return value of each `pushdown fn`, and the result of each closure; an
//! edge `A → B` means that the value of `A` may depend on the value of `B`.
//!
//! Sinks are the values the story observably depends on: conditions (`if`, `while`, `if let`,
//! `match` scrutinees and guards, `for` iterables, `?` operands), the arguments of steps
//! (`push_step`, which also evaluates choice guards), of `pushdown` builtins such as
//! `Meta::write`, and of calls through pointers or closures, and the ending an entry scene
//! returns to the system. Everything a sink depends on, transitively, is influential; a
//! game story variable that is not is reported.
//!
//! Because every condition is a sink, control dependence adds nothing to the influential set
//! and is not recorded. A `&mut` reference binds both ways: whatever is written through a
//! local or parameter of reference type flows back into the places it was bound to.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::db::Db;
use crate::def_map::{CrateId, def_maps};
use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::hir::{
    ArrayExpr, Condition, Expr, ExprId, ExprStore, Pat, PatId, Stmt, UnaryOp, file_hir,
};
use crate::infer::{InferenceResult, ValueRef, inference};
use crate::input::Program;
use crate::item_tree::{FnKind, ItemId, ItemKind, item_tree};
use crate::resolve::{BodyResolution, LocalId, file_resolution};
use crate::signatures::{ProgramSignatures, program_signatures};
use crate::ty::{FnDefId, Ty, TyNode};

pub(super) fn check(db: &dyn Db, program: Program) -> Vec<Diagnostic> {
    let maps = def_maps(db, program);
    let sigs = program_signatures(db, program);
    let mut graph = Graph::default();
    let mut story_vars = Vec::new();
    for &(file, krate) in &maps.files {
        let tree = item_tree(db, file);
        for (index, data) in tree.items.iter().enumerate() {
            let Ok(index) = u32::try_from(index) else {
                continue;
            };
            let item = ItemId { file, index };
            match data.kind {
                ItemKind::Static { mutable: true } if krate == CrateId::Game => {
                    story_vars.push(item);
                }
                ItemKind::Fn {
                    kind: FnKind::Pushdown,
                    has_body: true,
                    ..
                } => walk_fn(db, program, sigs, &mut graph, item),
                _ => {}
            }
        }
    }
    let influential = graph.influential();
    let mut out = Vec::new();
    for item in story_vars {
        let node = graph.ids.get(&Node::Story(item)).copied();
        if node.is_some_and(|node| influential.get(node).copied().unwrap_or(false)) {
            continue;
        }
        let Some(data) = item_tree(db, item.file).item(item.index) else {
            continue;
        };
        let name = data.name.as_deref().unwrap_or_default();
        let message = if graph.read.contains(&item) {
            format!(
                "the story variable `{name}` never influences the story: no condition, step, \
                 meta write or ending depends on its value"
            )
        } else if graph.written.contains(&item) {
            format!("the story variable `{name}` is never read: assignments to it have no effect")
        } else {
            format!("the story variable `{name}` is never used")
        };
        out.push(Diagnostic::new(
            item.file.path(db).clone(),
            data.name_range,
            DiagnosticCode::AnalysisUnusedStoryVariable,
            message,
        ));
    }
    out
}

/// A value of the dependence graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Node {
    /// A `static mut` item.
    Story(ItemId),
    /// A local of a body (closure parameters and `self` included).
    Local(ItemId, LocalId),
    /// Parameter `index` of a `pushdown fn`; `self` is index 0 when present.
    Param(ItemId, u32),
    /// The return value of a `pushdown fn`.
    Ret(ItemId),
    /// The result of a closure.
    Closure(ItemId, ExprId),
}

/// The nodes a value depends on.
type Deps = Vec<usize>;

#[derive(Default)]
struct Graph {
    ids: FxHashMap<Node, usize>,
    nodes: Vec<Node>,
    /// For each node, the nodes its value depends on.
    deps: Vec<Vec<usize>>,
    sinks: Vec<usize>,
    /// Story variables read by a path expression (not as an assignment target).
    read: FxHashSet<ItemId>,
    /// Story variables assigned to.
    written: FxHashSet<ItemId>,
}

impl Graph {
    fn node(&mut self, node: Node) -> usize {
        if let Some(&id) = self.ids.get(&node) {
            return id;
        }
        let id = self.nodes.len();
        self.ids.insert(node, id);
        self.nodes.push(node);
        self.deps.push(Vec::new());
        id
    }

    fn depend(&mut self, to: usize, from: &[usize]) {
        if let Some(deps) = self.deps.get_mut(to) {
            deps.extend(from.iter().copied().filter(|&from| from != to));
        }
    }

    fn sink(&mut self, from: &[usize]) {
        self.sinks.extend_from_slice(from);
    }

    /// Whether each node is backward-reachable from a sink.
    fn influential(&self) -> Vec<bool> {
        let mut seen = vec![false; self.nodes.len()];
        let mut stack = self.sinks.clone();
        while let Some(node) = stack.pop() {
            let Some(flag) = seen.get_mut(node) else {
                continue;
            };
            if *flag {
                continue;
            }
            *flag = true;
            if let Some(deps) = self.deps.get(node) {
                stack.extend(deps.iter().copied());
            }
        }
        seen
    }
}

fn walk_fn(
    db: &dyn Db,
    program: Program,
    sigs: &ProgramSignatures,
    graph: &mut Graph,
    item: ItemId,
) {
    let Some(Some(body)) = file_hir(db, item.file).bodies.get(item.index as usize) else {
        return;
    };
    let Some(Some(resolution)) = file_resolution(db, program, item.file)
        .items
        .get(item.index as usize)
    else {
        return;
    };
    let Some(res) = &resolution.body else {
        return;
    };
    let Some(infer) = inference(db, program, item) else {
        return;
    };
    let mut walker = Walker {
        db,
        sigs,
        graph,
        item,
        store: &body.store,
        res,
        infer,
        returns: Vec::new(),
        breaks: FxHashMap::default(),
    };
    let ret = walker.graph.node(Node::Ret(item));
    if sigs.fn_sig(item).is_some_and(|sig| sig.entry && sig.scene) {
        // The ending goes to the system.
        walker.graph.sink(&[ret]);
    }
    let mut position: u32 = 0;
    if body.has_self {
        if let Some((local, _)) = res.locals.iter().find(|(_, local)| local.pat.is_none()) {
            let param = walker.graph.node(Node::Param(item, 0));
            walker.bind(Node::Local(item, local), &[param]);
        }
        position = 1;
    }
    for &pat in &body.params {
        let param = walker.graph.node(Node::Param(item, position));
        walker.bind_pat(pat, &[param]);
        position = position.saturating_add(1);
    }
    walker.returns.push(ret);
    let value = walker.expr(body.value);
    walker.graph.depend(ret, &value);
}

struct Walker<'a> {
    db: &'a dyn Db,
    sigs: &'a ProgramSignatures,
    graph: &'a mut Graph,
    item: ItemId,
    store: &'a ExprStore,
    res: &'a BodyResolution,
    infer: &'a InferenceResult,
    /// Where `return` sends its value: the function's `Ret`, or the innermost closure's node.
    returns: Vec<usize>,
    /// The values of `break` expressions, by the loop they leave.
    breaks: FxHashMap<ExprId, Deps>,
}

impl Walker<'_> {
    /// `to` takes the value `from`, as by `let`, a pattern or argument passing. A node of
    /// `&mut` type aliases what it is bound to, so writes through it flow back.
    fn bind(&mut self, to: Node, from: &[usize]) {
        let to_id = self.graph.node(to);
        self.graph.depend(to_id, from);
        if self.has_mut_ref(to) {
            for &from in from {
                self.graph.depend(from, &[to_id]);
            }
        }
    }

    fn has_mut_ref(&self, node: Node) -> bool {
        let ty = match node {
            Node::Local(item, local) if item == self.item => self.infer.local_types.get(local),
            Node::Param(item, index) => self
                .sigs
                .fn_sig(item)
                .and_then(|sig| sig.params.get(index as usize)),
            _ => None,
        };
        ty.is_some_and(contains_mut_ref)
    }

    fn bind_pat(&mut self, pat: PatId, from: &[usize]) {
        let store = self.store;
        match store.pat(pat) {
            Pat::Bind { .. } => {
                if let Some(&local) = self.res.bindings.get(&pat) {
                    self.bind(Node::Local(self.item, local), from);
                }
            }
            Pat::Tuple(pats) | Pat::TupleStruct { pats, .. } | Pat::Or(pats) => {
                for &sub in pats {
                    self.bind_pat(sub, from);
                }
            }
            Pat::Record { fields, .. } => {
                for &(_, sub) in fields {
                    self.bind_pat(sub, from);
                }
            }
            Pat::Missing
            | Pat::Wild
            | Pat::Path(_)
            | Pat::Lit { .. }
            | Pat::Range { .. }
            | Pat::Rest => {}
        }
    }

    fn is_story_var(&self, item: ItemId) -> bool {
        matches!(
            item_tree(self.db, item.file)
                .item(item.index)
                .map(|data| &data.kind),
            Some(ItemKind::Static { mutable: true })
        )
    }

    fn sink_cond(&mut self, cond: Condition) {
        match cond {
            Condition::Expr(expr) => {
                let deps = self.expr(expr);
                self.graph.sink(&deps);
            }
            Condition::Let { pat, expr } => {
                let deps = self.expr(expr);
                self.graph.sink(&deps);
                self.bind_pat(pat, &deps);
            }
        }
    }

    fn union(&mut self, exprs: &[ExprId]) -> Deps {
        let mut out = Deps::new();
        for &expr in exprs {
            out.extend(self.expr(expr));
        }
        out
    }

    /// Records the effects of `expr` and returns the nodes its value depends on.
    fn expr(&mut self, expr: ExprId) -> Deps {
        let store = self.store;
        let mut out = match store.expr(expr) {
            Expr::Missing | Expr::Literal(_) | Expr::Continue { .. } => Deps::new(),
            Expr::Path(_) => self.path(expr),
            Expr::Block { stmts, tail, .. } => {
                for stmt in stmts {
                    match *stmt {
                        Stmt::Let {
                            pat,
                            init: Some(init),
                            ..
                        } => {
                            let deps = self.expr(init);
                            self.bind_pat(pat, &deps);
                        }
                        Stmt::Expr { expr, .. } => {
                            self.expr(expr);
                        }
                        Stmt::Let { init: None, .. } | Stmt::Item(_) => {}
                    }
                }
                match tail {
                    Some(tail) => self.expr(*tail),
                    None => Deps::new(),
                }
            }
            Expr::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.sink_cond(*cond);
                let mut out = self.expr(*then_branch);
                if let Some(else_branch) = else_branch {
                    out.extend(self.expr(*else_branch));
                }
                out
            }
            Expr::Match { scrutinee, arms } => {
                let scrutinee = self.expr(*scrutinee);
                self.graph.sink(&scrutinee);
                let mut out = Deps::new();
                for arm in arms {
                    self.bind_pat(arm.pat, &scrutinee);
                    if let Some(guard) = arm.guard {
                        let guard = self.expr(guard);
                        self.graph.sink(&guard);
                    }
                    out.extend(self.expr(arm.body));
                }
                out
            }
            Expr::Loop { body, .. } => {
                self.expr(*body);
                self.breaks.remove(&expr).unwrap_or_default()
            }
            Expr::While { cond, body, .. } => {
                self.sink_cond(*cond);
                self.expr(*body);
                self.breaks.remove(&expr).unwrap_or_default()
            }
            Expr::For {
                pat,
                iterable,
                body,
                ..
            } => {
                let iterable = self.expr(*iterable);
                self.graph.sink(&iterable);
                self.bind_pat(*pat, &iterable);
                self.expr(*body);
                self.breaks.remove(&expr).unwrap_or_default()
            }
            Expr::Break { value, .. } => {
                if let Some(value) = value {
                    let deps = self.expr(*value);
                    if let Some(&target) = self.res.loop_targets.get(&expr) {
                        self.breaks.entry(target).or_default().extend(deps);
                    }
                }
                Deps::new()
            }
            Expr::Return { value } => {
                if let Some(value) = value {
                    let deps = self.expr(*value);
                    if let Some(&target) = self.returns.last() {
                        self.graph.depend(target, &deps);
                    }
                }
                Deps::new()
            }
            Expr::Call { callee, args } => self.call(*callee, args),
            Expr::MethodCall { receiver, args, .. } => self.method_call(expr, *receiver, args),
            Expr::Field { base, .. }
            | Expr::Cast { expr: base, .. }
            | Expr::Unary { expr: base, .. }
            | Expr::Ref { expr: base, .. } => self.expr(*base),
            Expr::Try(base) => {
                // `?` decides whether the function returns early.
                let deps = self.expr(*base);
                self.graph.sink(&deps);
                deps
            }
            Expr::Index { base, index } => self.union(&[*base, *index]),
            Expr::Binary { lhs, rhs, .. } => self.union(&[*lhs, *rhs]),
            Expr::Assign { target, value, .. } => {
                // A compound assignment `x op= v` keeps depending on `x` itself, which adds
                // nothing to the graph; it is not a read of `x`.
                let value = self.expr(*value);
                self.assign(*target, &value);
                Deps::new()
            }
            Expr::Range { start, end, .. } => {
                let parts: Vec<ExprId> = start.iter().chain(end.iter()).copied().collect();
                self.union(&parts)
            }
            Expr::Tuple(items) | Expr::Array(ArrayExpr::List(items)) => self.union(items),
            Expr::Array(ArrayExpr::Repeat { value, count }) => self.union(&[*value, *count]),
            Expr::Record { fields, .. } => {
                let parts: Vec<ExprId> = fields.iter().map(|&(_, value)| value).collect();
                self.union(&parts)
            }
            Expr::Closure { body, .. } => {
                // The parameters are locals of this body that unknown callers bind; the
                // result flows wherever the closure value is called.
                let node = self.graph.node(Node::Closure(self.item, expr));
                self.returns.push(node);
                let value = self.expr(*body);
                self.returns.pop();
                self.graph.depend(node, &value);
                let mut out = vec![node];
                if let Some(info) = self.infer.closures.get(&expr) {
                    for &local in &info.captures {
                        out.push(self.graph.node(Node::Local(self.item, local)));
                    }
                }
                out
            }
        };
        out.sort_unstable();
        out.dedup();
        out
    }

    /// A path in value position.
    fn path(&mut self, expr: ExprId) -> Deps {
        match self.infer.paths.get(&expr) {
            Some(ValueRef::Local(local)) => vec![self.graph.node(Node::Local(self.item, *local))],
            Some(ValueRef::Static(item)) if self.is_story_var(*item) => {
                self.graph.read.insert(*item);
                vec![self.graph.node(Node::Story(*item))]
            }
            // A `pushdown fn` used as a value: calling it yields its return value.
            Some(ValueRef::Fn(fn_ref)) => match fn_ref.def {
                FnDefId::Item(item) if self.has_pushdown_body(item) => {
                    vec![self.graph.node(Node::Ret(item))]
                }
                _ => Deps::new(),
            },
            _ => Deps::new(),
        }
    }

    fn has_pushdown_body(&self, item: ItemId) -> bool {
        matches!(
            item_tree(self.db, item.file)
                .item(item.index)
                .map(|data| &data.kind),
            Some(ItemKind::Fn {
                kind: FnKind::Pushdown,
                has_body: true,
                ..
            })
        )
    }

    /// `target = value` (or `target op= value`).
    fn assign(&mut self, target: ExprId, value: &[usize]) {
        let mut extra = Deps::new();
        match self.place_root(target, &mut extra) {
            Some(root) => {
                extra.extend_from_slice(value);
                self.graph.depend(root, &extra);
            }
            None => {
                // Not a place rooted in a variable: every value the target depends on may be
                // the one written.
                let targets = self.expr(target);
                for node in targets {
                    self.graph.depend(node, value);
                }
            }
        }
    }

    /// The variable a place expression is rooted in, through fields, indexes and derefs;
    /// the dependencies of its index expressions go to `extra`.
    fn place_root(&mut self, expr: ExprId, extra: &mut Deps) -> Option<usize> {
        let store = self.store;
        match store.expr(expr) {
            Expr::Path(_) => match self.infer.paths.get(&expr)? {
                ValueRef::Local(local) => Some(self.graph.node(Node::Local(self.item, *local))),
                ValueRef::Static(item) if self.is_story_var(*item) => {
                    self.graph.written.insert(*item);
                    Some(self.graph.node(Node::Story(*item)))
                }
                _ => None,
            },
            Expr::Field { base, .. }
            | Expr::Unary {
                op: UnaryOp::Deref,
                expr: base,
            } => self.place_root(*base, extra),
            Expr::Index { base, index } => {
                let root = self.place_root(*base, extra)?;
                extra.extend(self.expr(*index));
                Some(root)
            }
            _ => None,
        }
    }

    fn call(&mut self, callee: ExprId, args: &[ExprId]) -> Deps {
        let store = self.store;
        let def = match self.infer.paths.get(&callee) {
            Some(ValueRef::Fn(fn_ref)) => Some(fn_ref.def),
            _ => match self.infer.expr_types.get(callee) {
                Some(Ty::FnDef(fn_ref)) => Some(fn_ref.def),
                _ => None,
            },
        };
        let callee_deps = match (def, store.expr(callee)) {
            // A named function is not a value read here.
            (Some(_), Expr::Path(_)) => Deps::new(),
            _ => self.expr(callee),
        };
        let arg_deps: Vec<Deps> = args.iter().map(|&arg| self.expr(arg)).collect();
        match def {
            Some(def) => {
                let mut out = self.apply(def, &arg_deps);
                out.extend(callee_deps);
                out
            }
            None => {
                // A pointer or closure: the target is unknown.
                let mut all = callee_deps;
                all.extend(arg_deps.into_iter().flatten());
                self.graph.sink(&all);
                all
            }
        }
    }

    fn method_call(&mut self, expr: ExprId, receiver: ExprId, args: &[ExprId]) -> Deps {
        let mut arg_deps = vec![self.expr(receiver)];
        arg_deps.extend(args.iter().map(|&arg| self.expr(arg)));
        match self.infer.method_calls.get(&expr).map(|call| call.func.def) {
            Some(def) => self.apply(def, &arg_deps),
            None => {
                let all: Deps = arg_deps.into_iter().flatten().collect();
                self.graph.sink(&all);
                all
            }
        }
    }

    /// A call of a known function with the dependencies of each argument.
    fn apply(&mut self, def: FnDefId, args: &[Deps]) -> Deps {
        let all: Deps = args.iter().flatten().copied().collect();
        let FnDefId::Item(item) = def else {
            // A tuple constructor.
            return all;
        };
        if Some(item) == self.sigs.std.push_step {
            // A step: the player sees its arguments (choice guards included); its result is
            // the player's input.
            self.graph.sink(&all);
            return Deps::new();
        }
        let Some(sig) = self.sigs.fn_sig(item) else {
            return all;
        };
        match sig.kind {
            FnKind::Pushdown if self.has_pushdown_body(item) => {
                for (index, deps) in args.iter().enumerate() {
                    let Ok(index) = u32::try_from(index) else {
                        continue;
                    };
                    self.bind(Node::Param(item, index), deps);
                }
                vec![self.graph.node(Node::Ret(item))]
            }
            FnKind::Pushdown => {
                // A `pushdown` builtin such as `Meta::write`: observable outside the story.
                self.graph.sink(&all);
                all
            }
            FnKind::Const | FnKind::Plain => {
                // The result depends on the arguments; a `&mut` argument may be written with
                // any of them.
                for (param, deps) in sig.params.iter().zip(args) {
                    if contains_mut_ref(param) {
                        for &node in deps {
                            self.graph.depend(node, &all);
                        }
                    }
                }
                all
            }
        }
    }
}

fn contains_mut_ref(ty: &Ty) -> bool {
    let mut found = false;
    ty.walk(&mut |node| {
        found |= matches!(node, TyNode::Ty(Ty::Ref { mutable: true, .. }));
    });
    found
}
