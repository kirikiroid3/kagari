//! Story variables that never influence the story (`docs/syntax/src/compilation.md`, stage 5).
//!
//! A flow-insensitive dependence graph over the MIR of every `pushdown fn` of the program (game
//! and std), closures included. Its nodes are the story variables, the locals of each body,
//! and the parameters and the return value of each `pushdown fn`; an edge `A → B` means that
//! the value of `A` may depend on the value of `B`. Each item contributes a cached
//! [`InfluenceSummary`]; [`check`] unions them.
//!
//! Sinks are the values the story observably depends on: branch conditions (`if`, `while`,
//! patterns, guards, `for` bounds, `?`), the arguments of steps, of `pushdown` builtins such as
//! `Meta::write`, and of calls through pointers or closures, and the ending an entry scene
//! returns to the system. Everything a sink depends on, transitively, is influential; a game
//! story variable that is not is reported. The branch of `&&`/`||` only decides the result
//! local and is not a sink.
//!
//! Because every condition is a sink, control dependence adds nothing to the influential set
//! and is not recorded. A `&mut` reference binds both ways: whatever is written through a
//! local or parameter of reference type flows back into the places it was bound to.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::db::Db;
use crate::def_map::{CrateId, def_maps};
use crate::diagnostic::{Anchor, Diag, DiagnosticCode};
use crate::hir::{Expr, ExprId};
use crate::input::Program;
use crate::item_tree::{FnKind, ItemId, ItemIndex, ItemKind, item_tree};
use crate::mir::{
    Body, Callee, ConstOperand, ItemMir, Local, MirSource, Operand, Place, PlaceBase, Proj, Rvalue,
    StatementKind, Terminator, item_mir,
};
use crate::signatures::{ProgramSignatures, program_signatures};
use crate::ty::{FnDefId, FnRef, Ty, TyKind, TyNode};

pub(super) fn check(db: &dyn Db, program: Program) -> Vec<Diag> {
    let maps = def_maps(db, program);
    let mut graph = Graph::default();
    let mut story_vars = Vec::new();
    for &(file, krate) in &maps.files {
        let tree = item_tree(db, file);
        for (index, data) in tree.items.iter().enumerate() {
            let Some(index) = ItemIndex::from_usize(index) else {
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
                } => graph.add(influence_summary(db, program, item)),
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
        out.push(Diag::new(
            Anchor::ItemName(item),
            DiagnosticCode::AnalysisUnusedStoryVariable,
            message,
        ));
    }
    out
}

/// A value of the dependence graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Node {
    /// A `static mut` item.
    Story(ItemId),
    /// A local of a MIR body.
    Local(MirSource, Local),
    /// Parameter `index` of a `pushdown fn`; `self` is index 0 when present.
    Param(ItemId, u32),
    /// The return value of a `pushdown fn`.
    Ret(ItemId),
}

/// The dependence edges, sinks and story-variable uses of one `pushdown fn`, closures included.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct InfluenceSummary {
    /// `(to, from)`: the value of `to` may depend on the value of `from`.
    pub edges: Vec<(Node, Node)>,
    pub sinks: Vec<Node>,
    /// Story variables read (not only as an assignment target).
    pub read: Vec<ItemId>,
    /// Story variables assigned to.
    pub written: Vec<ItemId>,
}

#[salsa::tracked(returns(ref))]
pub(crate) fn influence_summary(db: &dyn Db, program: Program, item: ItemId) -> InfluenceSummary {
    let Some(mir) = item_mir(db, program, item).as_ref() else {
        return InfluenceSummary::default();
    };
    let sigs = program_signatures(db, program);
    let mut cx = Summarizer {
        db,
        sigs,
        item,
        src: MirSource::Item(item),
        compound: None,
        out: InfluenceSummary::default(),
        read: FxHashSet::default(),
        written: FxHashSet::default(),
    };
    cx.item_mir(mir);
    let mut out = cx.out;
    // FxHash iteration order is deterministic for the same insertions.
    out.read = cx.read.into_iter().collect();
    out.written = cx.written.into_iter().collect();
    out
}

#[derive(Default)]
struct Graph {
    ids: FxHashMap<Node, usize>,
    /// For each node, the nodes its value depends on.
    deps: Vec<Vec<usize>>,
    sinks: Vec<usize>,
    read: FxHashSet<ItemId>,
    written: FxHashSet<ItemId>,
}

impl Graph {
    fn node(&mut self, node: Node) -> usize {
        if let Some(&id) = self.ids.get(&node) {
            return id;
        }
        let id = self.deps.len();
        self.ids.insert(node, id);
        self.deps.push(Vec::new());
        id
    }

    fn add(&mut self, summary: &InfluenceSummary) {
        for &(to, from) in &summary.edges {
            let to = self.node(to);
            let from = self.node(from);
            if let Some(deps) = self.deps.get_mut(to) {
                deps.push(from);
            }
        }
        for &sink in &summary.sinks {
            let sink = self.node(sink);
            self.sinks.push(sink);
        }
        self.read.extend(summary.read.iter().copied());
        self.written.extend(summary.written.iter().copied());
    }

    /// Whether each node is backward-reachable from a sink.
    fn influential(&self) -> Vec<bool> {
        let mut seen = vec![false; self.deps.len()];
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

/// The nodes a value depends on.
type Deps = Vec<Node>;

struct Summarizer<'a> {
    db: &'a dyn Db,
    sigs: &'a ProgramSignatures,
    item: ItemId,
    /// The body being summarized.
    src: MirSource,
    /// Whether the statement being summarized lowers a compound assignment `x op= v`, whose
    /// read of `x` is not a read of a story variable.
    compound: Option<ExprId>,
    out: InfluenceSummary,
    read: FxHashSet<ItemId>,
    written: FxHashSet<ItemId>,
}

impl Summarizer<'_> {
    fn item_mir(&mut self, mir: &ItemMir) {
        let ret = Node::Ret(self.item);
        if self
            .sigs
            .fn_sig(self.item)
            .is_some_and(|sig| sig.entry && sig.scene)
        {
            // The ending goes to the system.
            self.out.sinks.push(ret);
        }
        self.src = mir.main.source;
        for (position, arg) in (0u32..).zip(mir.main.args()) {
            let mut_ref = mir
                .main
                .local_decl(arg)
                .is_some_and(|decl| contains_mut_ref(&decl.ty));
            self.bind(
                self.local(arg),
                &[Node::Param(self.item, position)],
                mut_ref,
            );
        }
        self.depend(ret, &[self.local(Body::RETURN)]);
        self.body(&mir.main);
        for (_, closure) in &mir.closures {
            self.src = closure.source;
            self.body(closure);
        }
    }

    fn local(&self, local: Local) -> Node {
        Node::Local(self.src, local)
    }

    fn depend(&mut self, to: Node, from: &[Node]) {
        self.out.edges.extend(
            from.iter()
                .filter(|&&from| from != to)
                .map(|&from| (to, from)),
        );
    }

    /// `to` takes the value `from`; a `&mut` node aliases what it is bound to, so writes
    /// through it flow back.
    fn bind(&mut self, to: Node, from: &[Node], mut_ref: bool) {
        self.depend(to, from);
        if mut_ref {
            for &from in from {
                self.depend(from, &[to]);
            }
        }
    }

    fn body(&mut self, body: &Body) {
        let hir = crate::hir::body_of(self.db, self.src.owner());
        for (_, block) in body.blocks.iter() {
            for stmt in &block.stmts {
                self.compound = stmt.src.filter(|&src| {
                    hir.is_some_and(|hir| {
                        matches!(hir.store.expr(src), Expr::Assign { op: Some(_), .. })
                    })
                });
                match &stmt.kind {
                    StatementKind::Assign(place, rvalue) => self.assign(body, place, rvalue),
                    StatementKind::Call { dest, callee, args } => {
                        let value = self.call(callee, args);
                        self.write(dest, &value);
                    }
                }
            }
            self.compound = None;
            self.terminator(&block.term);
        }
    }

    fn terminator(&mut self, term: &Terminator) {
        match term {
            Terminator::Goto(_) | Terminator::Return | Terminator::Unreachable => {}
            Terminator::SwitchBool {
                cond,
                short_circuit,
                ..
            } => {
                let deps = self.operand(cond);
                match short_circuit {
                    // `&&`/`||` only decides its result.
                    Some(result) => self.depend(self.local(*result), &deps),
                    None => self.out.sinks.extend(deps),
                }
            }
            Terminator::SwitchVariant { place, .. } => {
                let deps = self.read_place(place);
                self.out.sinks.extend(deps);
            }
            Terminator::Step { arg, dest, .. } => {
                // The player sees the step (choice guards included); its result is the
                // player's input.
                let deps = self.operand(arg);
                self.out.sinks.extend(deps);
                self.write(dest, &[]);
            }
            Terminator::Rng { dest, .. } => self.write(dest, &[]),
            Terminator::SceneCall {
                callee, args, dest, ..
            } => {
                let value = self.call(callee, args);
                self.write(dest, &value);
            }
        }
    }

    fn assign(&mut self, body: &Body, place: &Place, rvalue: &Rvalue) {
        let value = self.rvalue(rvalue);
        // A bare local of `&mut` type (a binding, a temp of `&mut x`) aliases its source.
        let mut_ref = place
            .as_local()
            .and_then(|local| body.local_decl(local))
            .is_some_and(|decl| contains_mut_ref(&decl.ty));
        if mut_ref && let Some(local) = place.as_local() {
            self.bind(self.local(local), &value, true);
        } else {
            self.write(place, &value);
        }
    }

    /// A write of a value depending on `value` to `place`: its root depends on the value and
    /// on the place's index locals.
    fn write(&mut self, place: &Place, value: &[Node]) {
        let root = match place.base {
            PlaceBase::Local(local) => self.local(local),
            PlaceBase::StoryVar(item) => {
                self.written.insert(item);
                Node::Story(item)
            }
        };
        let mut deps = self.index_deps(place);
        deps.extend_from_slice(value);
        self.depend(root, &deps);
    }

    fn index_deps(&self, place: &Place) -> Deps {
        place
            .proj
            .iter()
            .filter_map(|proj| match proj {
                Proj::Index(local) => Some(self.local(*local)),
                Proj::Deref | Proj::Field(_) | Proj::Downcast(_) => None,
            })
            .collect()
    }

    /// The nodes a read of `place` depends on: its root and its index locals.
    fn read_place(&mut self, place: &Place) -> Deps {
        let mut deps = self.index_deps(place);
        deps.push(match place.base {
            PlaceBase::Local(local) => self.local(local),
            PlaceBase::StoryVar(item) => {
                self.read.insert(item);
                Node::Story(item)
            }
        });
        deps
    }

    fn operand(&mut self, operand: &Operand) -> Deps {
        match operand {
            Operand::Copy(place) => self.read_place(place),
            // A `pushdown fn` used as a value: calling it yields its return value.
            Operand::Const(ConstOperand::Fn(FnRef {
                def: FnDefId::Item(item),
                ..
            })) if self.has_pushdown_body(*item) => vec![Node::Ret(*item)],
            Operand::Const(_) => Deps::new(),
        }
    }

    fn operands<'o>(&mut self, operands: impl IntoIterator<Item = &'o Operand>) -> Deps {
        let mut out = Deps::new();
        for operand in operands {
            out.extend(self.operand(operand));
        }
        out
    }

    /// The left operand of a compound assignment's operation reads the target, which is not
    /// a read of a story variable.
    fn binary(&mut self, lhs: &Operand, rhs: &Operand) -> Deps {
        let mut out = match (self.compound, lhs) {
            (Some(_), Operand::Copy(place)) => {
                let mut deps = self.index_deps(place);
                deps.push(match place.base {
                    PlaceBase::Local(local) => self.local(local),
                    PlaceBase::StoryVar(item) => Node::Story(item),
                });
                deps
            }
            _ => self.operand(lhs),
        };
        out.extend(self.operand(rhs));
        out
    }

    fn rvalue(&mut self, rvalue: &Rvalue) -> Deps {
        match rvalue {
            Rvalue::Use(operand)
            | Rvalue::IntNeg { operand, .. }
            | Rvalue::IntNot { operand, .. }
            | Rvalue::IntClamp { operand, .. }
            | Rvalue::BoolNot(operand)
            | Rvalue::DecNeg(operand)
            | Rvalue::RatNeg(operand)
            | Rvalue::Repeat { operand, .. }
            | Rvalue::Cast { operand, .. } => self.operand(operand),
            Rvalue::IntBin { lhs, rhs, .. }
            | Rvalue::BoolBin { lhs, rhs, .. }
            | Rvalue::DecBin { lhs, rhs, .. }
            | Rvalue::RatBin { lhs, rhs, .. } => self.binary(lhs, rhs),
            Rvalue::Cmp { lhs, rhs, .. } => self.operands([lhs, rhs]),
            Rvalue::Aggregate { ops, .. } => self.operands(ops),
            Rvalue::Ref { place, .. } | Rvalue::Len(place) => self.read_place(place),
            Rvalue::Closure { expr, captures } => {
                // The parameters are locals that unknown callers bind; the result flows
                // wherever the closure value is called. A capture is the captured place.
                let closure = MirSource::Closure {
                    owner: self.src.owner(),
                    expr: *expr,
                };
                let mut out = vec![Node::Local(closure, Body::RETURN)];
                for (index, capture) in (1u32..).zip(captures) {
                    let deps = self.operand(capture);
                    let arg =
                        Node::Local(closure, Local::from_raw(la_arena::RawIdx::from_u32(index)));
                    self.bind(arg, &deps, true);
                    out.extend(deps);
                }
                out
            }
        }
    }

    /// A call; returns what its result depends on.
    fn call(&mut self, callee: &Callee, args: &[Operand]) -> Deps {
        match callee {
            Callee::Fn(fn_ref) | Callee::Builtin(_, fn_ref) => {
                let args: Vec<Deps> = args.iter().map(|arg| self.operand(arg)).collect();
                self.apply(fn_ref.def, &args)
            }
            Callee::Value(callee) => {
                // A pointer or closure: the target is unknown.
                let mut all = self.operand(callee);
                all.extend(self.operands(args));
                self.out.sinks.extend(all.iter().copied());
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
            self.out.sinks.extend(all);
            return Deps::new();
        }
        let Some(sig) = self.sigs.fn_sig(item) else {
            return all;
        };
        match sig.kind {
            FnKind::Pushdown if self.has_pushdown_body(item) => {
                for ((index, deps), param) in (0u32..).zip(args).zip(sig.params.iter()) {
                    self.bind(Node::Param(item, index), deps, contains_mut_ref(param));
                }
                vec![Node::Ret(item)]
            }
            FnKind::Pushdown => {
                // A `pushdown` builtin such as `Meta::write`: observable outside the story.
                self.out.sinks.extend(all.iter().copied());
                all
            }
            FnKind::Const | FnKind::Plain => {
                // The result depends on the arguments; a `&mut` argument may be written with
                // any of them.
                for (param, deps) in sig.params.iter().zip(args) {
                    if contains_mut_ref(param) {
                        for &node in deps {
                            self.depend(node, &all);
                        }
                    }
                }
                all
            }
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
}

fn contains_mut_ref(ty: &Ty) -> bool {
    let mut found = false;
    ty.walk(&mut |node| {
        found |=
            matches!(node, TyNode::Ty(t) if matches!(t.kind(), TyKind::Ref { mutable: true, .. }));
    });
    found
}
