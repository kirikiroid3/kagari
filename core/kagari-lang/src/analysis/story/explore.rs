//! The reachable states of the story, from every entry scene and every argument value.
//!
//! Scenes are summarized per *context*: the scene with its arguments and the story
//! variables at entry. A context's states are its suspension points (steps and scene
//! calls) with the live slots of its frame and the story variables; its exits are the
//! values it can return with the story variables (and the final values of its `&mut`
//! parameters) at return. A call of a context resumes the caller once per exit, as
//! recursive state machines are explored: a scene called from many places with the same
//! entry state is explored once.

use std::hash::Hash;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::db::Db;
use crate::def_map::def_maps;
use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::eval::domain::{DomainCtx, product};
use crate::eval::interp::{Ctx, ErrorKind, EvalError};
use crate::eval::{AssetKind, ConstValue, TABLE_DOMAIN_CAP, const_eval, display_value};
use crate::hir::{Expr, ExprId};
use crate::infer::ValueRef;
use crate::item_tree::{ItemId, item_tree};
use crate::signatures::{FnSig, ItemSig};
use crate::ty::{FnDefId, FnRef};

use super::Scenes;
use super::cfg::{BlockId, Term};
use super::exec::{Scene, Snapshot, Stop, run};

pub(super) type CtxId = usize;
pub(super) type NodeId = usize;
pub(super) type ExitId = usize;

/// Values stored once and referred to by index.
pub(super) struct Interner<T> {
    items: Vec<T>,
    index: FxHashMap<T, u32>,
}

impl<T: Clone + Eq + Hash> Interner<T> {
    pub fn new() -> Interner<T> {
        Interner {
            items: Vec::new(),
            index: FxHashMap::default(),
        }
    }

    pub fn intern(&mut self, value: T) -> u32 {
        if let Some(id) = self.index.get(&value) {
            return *id;
        }
        let id = u32::try_from(self.items.len()).unwrap_or(u32::MAX);
        self.items.push(value.clone());
        self.index.insert(value, id);
        id
    }

    pub fn get(&self, id: u32) -> Option<&T> {
        self.items.get(id as usize)
    }
}

/// Where a resumption leads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Target {
    Node(NodeId),
    Exit(ExitId),
    /// The path stopped: an error, or a state the analysis could not continue from.
    Dead,
}

pub(super) enum NodeKind {
    /// A step with its render and, per outcome (one for `s!`, one per shown arm of a
    /// `choice!`), where the story goes next.
    Step {
        expr: ExprId,
        render: Option<ConstValue>,
        /// Whether the step is a `choice!`.
        choice: bool,
        succ: Vec<Target>,
    },
    /// A scene call: the callee's context and, per exit of it, where the caller goes next.
    Call {
        callee: CtxId,
        succ: Vec<(ExitId, Target)>,
    },
    /// A random draw (`rng_choose`): per value of the drawn type, where the story goes next.
    Rng { succ: Vec<Target> },
}

pub(super) struct Node {
    pub block: BlockId,
    pub frame: u32,
    pub globals: u32,
    pub kind: NodeKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct ExitData {
    pub ret: ConstValue,
    pub globals: u32,
    pub outs: Vec<ConstValue>,
}

pub(super) struct Context {
    /// Index into `Scenes::list`.
    pub scene: usize,
    pub fn_ref: FnRef,
    pub args: Vec<ConstValue>,
    pub globals: u32,
    pub entry: Target,
    pub nodes: Vec<Node>,
    node_index: FxHashMap<(BlockId, u32, u32), NodeId>,
    pub exits: Vec<ExitData>,
    exit_index: FxHashMap<ExitData, ExitId>,
    pub callers: Vec<(CtxId, NodeId)>,
}

pub(super) struct Explored {
    pub contexts: Vec<Context>,
    pub roots: Vec<CtxId>,
    pub frames: Interner<Vec<Option<ConstValue>>>,
    pub globals: Interner<Vec<ConstValue>>,
    pub vars: Vec<ItemId>,
    /// Every reachable state was visited: no path was given up and no cap was hit.
    pub complete: bool,
}

enum Task {
    Start(CtxId),
    Step {
        ctx: CtxId,
        node: NodeId,
        outcome: usize,
        value: ConstValue,
    },
    Return {
        ctx: CtxId,
        node: NodeId,
        exit: ExitId,
    },
}

struct Explorer<'a, 'c, 'db> {
    db: &'db dyn Db,
    ctx: &'c Ctx<'db>,
    scenes: &'a Scenes<'db>,
    out: Explored,
    ctx_index: FxHashMap<(FnRef, Vec<ConstValue>, u32), CtxId>,
    tasks: Vec<Task>,
    states: usize,
    /// Choices reached with every arm hidden: scene, step, story variables of a witness.
    stuck: FxHashMap<(usize, ExprId), u32>,
    faults: Vec<Diagnostic>,
    /// The inputs of the [`DomainCtx`] that enumerates argument values and random draws.
    assets: FxHashMap<AssetKind, Vec<ConstValue>>,
    functions: Vec<(ItemId, &'db FnSig)>,
}

/// Explores the story from every entry scene; `None` when there is nothing to explore.
pub(super) fn explore(
    db: &dyn Db,
    ctx: &Ctx<'_>,
    scenes: &Scenes<'_>,
    vars: &[ItemId],
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Explored> {
    let entries: Vec<usize> = scenes
        .list
        .iter()
        .enumerate()
        .filter(|(_, scene)| {
            scene.game
                && ctx.sigs.fn_sig(scene.item).is_some_and(|sig| {
                    sig.entry && sig.generics.params.is_empty() && sig.generics.parent_count == 0
                })
        })
        .map(|(index, _)| index)
        .collect();
    if entries.is_empty() {
        return None;
    }
    let mut initial = Vec::with_capacity(vars.len());
    for var in vars {
        // Errors in initializers are stage 4's.
        initial.push(ctx.static_value(*var).ok()?.value);
    }
    let mut explorer = Explorer {
        db,
        ctx,
        scenes,
        out: Explored {
            contexts: Vec::new(),
            roots: Vec::new(),
            frames: Interner::new(),
            globals: Interner::new(),
            vars: vars.to_vec(),
            complete: true,
        },
        ctx_index: FxHashMap::default(),
        tasks: Vec::new(),
        states: 0,
        stuck: FxHashMap::default(),
        faults: Vec::new(),
        assets: assets(db, ctx),
        functions: functions(db, ctx),
    };
    let globals = explorer.out.globals.intern(initial);
    let mut roots = Vec::new();
    for &scene in &entries {
        let Some(data) = scenes.list.get(scene) else {
            continue;
        };
        let params = ctx
            .sigs
            .fn_sig(data.item)
            .map(|sig| sig.params.clone())
            .unwrap_or_default();
        let mut size: u64 = 1;
        let mut domains = Vec::with_capacity(params.len());
        for ty in &params {
            let values = explorer.domain().values(ty, TABLE_DOMAIN_CAP);
            size = size.saturating_mul(values.as_ref().map_or(u64::MAX, |values| {
                u64::try_from(values.len()).unwrap_or(u64::MAX)
            }));
            domains.push(values.unwrap_or_default());
        }
        if size > TABLE_DOMAIN_CAP {
            explorer.out.complete = false;
            diagnostics.push(name_diagnostic(
                db,
                data.item,
                DiagnosticCode::AnalysisTooManyStates,
                format!(
                    "the entry scene `{}` takes too many argument values to analyze: more than \
                     the limit of {TABLE_DOMAIN_CAP}, so unreachable steps and scenes are not \
                     reported",
                    data.name(db)
                ),
            ));
            continue;
        }
        let fn_ref = FnRef {
            def: FnDefId::Item(data.item),
            self_ty: None,
            args: Vec::new(),
        };
        for args in product(&domains) {
            roots.push((scene, fn_ref.clone(), args));
        }
    }
    for (scene, fn_ref, args) in roots {
        let root = explorer.context(scene, fn_ref, args, globals);
        explorer.out.roots.push(root);
    }
    explorer.run(diagnostics, &entries);
    diagnostics.append(&mut explorer.faults);
    explorer.report_stuck(diagnostics);
    Some(explorer.out)
}

impl Explorer<'_, '_, '_> {
    fn domain(&self) -> DomainCtx<'_> {
        DomainCtx {
            sigs: self.ctx.sigs,
            std: self.ctx.std,
            texts: &[],
            assets: &self.assets,
            functions: &self.functions,
        }
    }

    /// The context of `scene` entered with `args` and `globals`, explored once.
    fn context(
        &mut self,
        scene: usize,
        fn_ref: FnRef,
        args: Vec<ConstValue>,
        globals: u32,
    ) -> CtxId {
        let key = (fn_ref, args, globals);
        if let Some(id) = self.ctx_index.get(&key) {
            return *id;
        }
        let id = self.out.contexts.len();
        let (fn_ref, args, globals) = key.clone();
        self.out.contexts.push(Context {
            scene,
            fn_ref,
            args,
            globals,
            entry: Target::Dead,
            nodes: Vec::new(),
            node_index: FxHashMap::default(),
            exits: Vec::new(),
            exit_index: FxHashMap::default(),
            callers: Vec::new(),
        });
        self.ctx_index.insert(key, id);
        self.tasks.push(Task::Start(id));
        id
    }

    fn run(&mut self, diagnostics: &mut Vec<Diagnostic>, entries: &[usize]) {
        let cap = self.ctx.program.options(self.db).state_cap;
        let limit = usize::try_from(cap).unwrap_or(usize::MAX);
        while let Some(task) = self.tasks.pop() {
            if self.states > limit {
                self.out.complete = false;
                if let Some(data) = entries
                    .first()
                    .and_then(|scene| self.scenes.list.get(*scene))
                {
                    diagnostics.push(name_diagnostic(
                        self.db,
                        data.item,
                        DiagnosticCode::AnalysisTooManyStates,
                        format!(
                            "the story has too many states to analyze: more than the limit of \
                             {cap}, so unreachable steps and scenes are not reported"
                        ),
                    ));
                }
                return;
            }
            match task {
                Task::Start(id) => {
                    let Some(context) = self.out.contexts.get(id) else {
                        continue;
                    };
                    let Some(scene) = self.scenes.list.get(context.scene) else {
                        continue;
                    };
                    let mut frame = vec![None; scene.body.slots.len()];
                    for (slot, arg) in scene.body.params.iter().zip(&context.args) {
                        if let Some(value) = frame.get_mut(*slot) {
                            *value = Some(arg.clone());
                        }
                    }
                    let entry = scene.body.entry;
                    let globals = context.globals;
                    let target = self.resume(id, entry, frame, globals);
                    if let Some(context) = self.out.contexts.get_mut(id) {
                        context.entry = target;
                    }
                }
                Task::Step {
                    ctx,
                    node,
                    outcome,
                    value,
                } => {
                    let Some((block, frame, globals)) = self.node_state(ctx, node) else {
                        continue;
                    };
                    let (dest, next) = match self.term(ctx, block) {
                        Some(Term::Step { dest, next, .. } | Term::Rng { dest, next, .. }) => {
                            (dest, next)
                        }
                        _ => continue,
                    };
                    let mut frame = frame;
                    if let Some(slot) = dest.and_then(|dest| frame.get_mut(dest)) {
                        *slot = Some(value);
                    }
                    let target = self.resume(ctx, next, frame, globals);
                    if let Some(Node {
                        kind: NodeKind::Step { succ, .. } | NodeKind::Rng { succ, .. },
                        ..
                    }) = self.node_mut(ctx, node)
                        && let Some(slot) = succ.get_mut(outcome)
                    {
                        *slot = target;
                    }
                }
                Task::Return { ctx, node, exit } => {
                    let Some((block, frame, _)) = self.node_state(ctx, node) else {
                        continue;
                    };
                    let Some(Term::Call {
                        dest, outs, next, ..
                    }) = self.term(ctx, block)
                    else {
                        continue;
                    };
                    let Some(callee) = self.callee(ctx, node) else {
                        continue;
                    };
                    let Some(data) = self
                        .out
                        .contexts
                        .get(callee)
                        .and_then(|callee| callee.exits.get(exit))
                        .cloned()
                    else {
                        continue;
                    };
                    let mut frame = frame;
                    if let Some(slot) = dest.and_then(|dest| frame.get_mut(dest)) {
                        *slot = Some(data.ret.clone());
                    }
                    for (slot, value) in outs.iter().zip(&data.outs) {
                        if let Some(slot) = frame.get_mut(*slot) {
                            *slot = Some(value.clone());
                        }
                    }
                    let target = self.resume(ctx, next, frame, data.globals);
                    if let Some(Node {
                        kind: NodeKind::Call { succ, .. },
                        ..
                    }) = self.node_mut(ctx, node)
                    {
                        succ.push((exit, target));
                    }
                }
            }
        }
    }

    fn node_state(
        &self,
        ctx: CtxId,
        node: NodeId,
    ) -> Option<(BlockId, Vec<Option<ConstValue>>, u32)> {
        let node = self.out.contexts.get(ctx)?.nodes.get(node)?;
        let frame = self.out.frames.get(node.frame)?.clone();
        Some((node.block, frame, node.globals))
    }

    fn node_mut(&mut self, ctx: CtxId, node: NodeId) -> Option<&mut Node> {
        self.out.contexts.get_mut(ctx)?.nodes.get_mut(node)
    }

    fn callee(&self, ctx: CtxId, node: NodeId) -> Option<CtxId> {
        match self.out.contexts.get(ctx)?.nodes.get(node)?.kind {
            NodeKind::Call { callee, .. } => Some(callee),
            NodeKind::Step { .. } | NodeKind::Rng { .. } => None,
        }
    }

    fn term(&self, ctx: CtxId, block: BlockId) -> Option<Term> {
        let scene = self.out.contexts.get(ctx)?.scene;
        Some(
            self.scenes
                .list
                .get(scene)?
                .body
                .blocks
                .get(block)?
                .term
                .clone(),
        )
    }

    /// Runs context `ctx` from `block` to its next suspension and records where it got.
    fn resume(
        &mut self,
        ctx: CtxId,
        block: BlockId,
        frame: Vec<Option<ConstValue>>,
        globals: u32,
    ) -> Target {
        let Some(context) = self.out.contexts.get(ctx) else {
            return Target::Dead;
        };
        let Some(data) = self.scenes.list.get(context.scene) else {
            return Target::Dead;
        };
        let Some(globals) = self.out.globals.get(globals).cloned() else {
            return Target::Dead;
        };
        let env = self.ctx.env_for(data.item, context.fn_ref.args.clone());
        let scene = Scene {
            b: data.b,
            body: &data.body,
        };
        let (stop, snapshot) = run(
            self.ctx,
            &scene,
            env,
            block,
            &frame,
            &self.out.vars,
            &globals,
        );
        self.settle(ctx, stop, snapshot)
    }

    fn settle(&mut self, ctx: CtxId, stop: Stop, snapshot: Snapshot) -> Target {
        let Some(scene_index) = self.out.contexts.get(ctx).map(|context| context.scene) else {
            return Target::Dead;
        };
        match stop {
            Stop::Step { block, record } => {
                let frame = self.out.frames.intern(snapshot.frame);
                let globals = self.out.globals.intern(snapshot.globals);
                if let Some(node) = self
                    .out
                    .contexts
                    .get(ctx)
                    .and_then(|context| context.node_index.get(&(block, frame, globals)))
                {
                    return Target::Node(*node);
                }
                let Some(Term::Step { expr, .. }) = self.term(ctx, block) else {
                    return Target::Dead;
                };
                let Some((render, outcomes)) = self.step_outcomes(&record) else {
                    self.give_up(scene_index, Some(expr));
                    return Target::Dead;
                };
                if outcomes.is_empty() {
                    self.stuck.entry((scene_index, expr)).or_insert(globals);
                }
                let choice = matches!(&record, ConstValue::Adt { item, .. } if *item == self.ctx.std.choice.item);
                let id = self.add_node(
                    ctx,
                    Node {
                        block,
                        frame,
                        globals,
                        kind: NodeKind::Step {
                            expr,
                            render,
                            choice,
                            succ: vec![Target::Dead; outcomes.len()],
                        },
                    },
                );
                for (outcome, value) in outcomes.into_iter().enumerate() {
                    self.tasks.push(Task::Step {
                        ctx,
                        node: id,
                        outcome,
                        value,
                    });
                }
                Target::Node(id)
            }
            Stop::Rng { block, ty } => {
                let frame = self.out.frames.intern(snapshot.frame);
                let globals = self.out.globals.intern(snapshot.globals);
                if let Some(node) = self
                    .out
                    .contexts
                    .get(ctx)
                    .and_then(|context| context.node_index.get(&(block, frame, globals)))
                {
                    return Target::Node(*node);
                }
                let Some(Term::Rng { expr, .. }) = self.term(ctx, block) else {
                    return Target::Dead;
                };
                let Some(values) = self.domain().values(&ty, TABLE_DOMAIN_CAP) else {
                    self.out.complete = false;
                    if let Some(data) = self.scenes.list.get(scene_index) {
                        let diagnostic = data.diagnostic(
                            self.db,
                            expr,
                            DiagnosticCode::AnalysisTooManyStates,
                            format!(
                                "this draw has too many values to analyze: more than the limit \
                                 of {TABLE_DOMAIN_CAP}, so unreachable steps and scenes are not \
                                 reported"
                            ),
                        );
                        if !self.faults.contains(&diagnostic) {
                            self.faults.push(diagnostic);
                        }
                    }
                    return Target::Dead;
                };
                let id = self.add_node(
                    ctx,
                    Node {
                        block,
                        frame,
                        globals,
                        kind: NodeKind::Rng {
                            succ: vec![Target::Dead; values.len()],
                        },
                    },
                );
                for (outcome, value) in values.into_iter().enumerate() {
                    self.tasks.push(Task::Step {
                        ctx,
                        node: id,
                        outcome,
                        value,
                    });
                }
                Target::Node(id)
            }
            Stop::Call {
                block,
                callee,
                args,
            } => {
                let frame = self.out.frames.intern(snapshot.frame);
                let globals = self.out.globals.intern(snapshot.globals);
                if let Some(node) = self
                    .out
                    .contexts
                    .get(ctx)
                    .and_then(|context| context.node_index.get(&(block, frame, globals)))
                {
                    return Target::Node(*node);
                }
                let scene = match callee.def {
                    FnDefId::Item(item) => self.scenes.index.get(&item).copied(),
                    _ => None,
                };
                let Some(scene) = scene else {
                    self.give_up(scene_index, snapshot.last);
                    return Target::Dead;
                };
                let callee_ctx = self.context(scene, callee, args, globals);
                let id = self.add_node(
                    ctx,
                    Node {
                        block,
                        frame,
                        globals,
                        kind: NodeKind::Call {
                            callee: callee_ctx,
                            succ: Vec::new(),
                        },
                    },
                );
                let exits = match self.out.contexts.get_mut(callee_ctx) {
                    Some(callee) => {
                        callee.callers.push((ctx, id));
                        callee.exits.len()
                    }
                    None => 0,
                };
                for exit in 0..exits {
                    self.tasks.push(Task::Return {
                        ctx,
                        node: id,
                        exit,
                    });
                }
                Target::Node(id)
            }
            Stop::Return { value, outs } => {
                let globals = self.out.globals.intern(snapshot.globals);
                let data = ExitData {
                    ret: value,
                    globals,
                    outs,
                };
                let Some(context) = self.out.contexts.get_mut(ctx) else {
                    return Target::Dead;
                };
                if let Some(exit) = context.exit_index.get(&data) {
                    return Target::Exit(*exit);
                }
                let exit = context.exits.len();
                context.exits.push(data.clone());
                context.exit_index.insert(data, exit);
                let callers = context.callers.clone();
                for (caller, node) in callers {
                    self.tasks.push(Task::Return {
                        ctx: caller,
                        node,
                        exit,
                    });
                }
                Target::Exit(exit)
            }
            Stop::Fault { error, at } => {
                if let Some(scene) = self.scenes.list.get(scene_index)
                    && scene.game
                {
                    self.fault(scene_index, &error, at);
                }
                Target::Dead
            }
            Stop::Abort => {
                self.give_up(scene_index, snapshot.last);
                Target::Dead
            }
        }
    }

    /// The exploration cannot continue a path: some state may be missed, so nothing is
    /// claimed unreachable, and the place is reported.
    fn give_up(&mut self, scene: usize, at: Option<ExprId>) {
        self.out.complete = false;
        let Some(data) = self.scenes.list.get(scene) else {
            return;
        };
        let message = "the story analysis cannot follow the story past this point (it holds a \
                       value the analysis cannot store, such as a closure that captures \
                       locals, across a step), so unreachable steps and scenes are not reported";
        let diagnostic = match at {
            Some(expr) => {
                data.diagnostic(self.db, expr, DiagnosticCode::AnalysisIncomplete, message)
            }
            None => name_diagnostic(
                self.db,
                data.item,
                DiagnosticCode::AnalysisIncomplete,
                message.to_owned(),
            ),
        };
        if !self.faults.contains(&diagnostic) {
            self.faults.push(diagnostic);
        }
    }

    fn add_node(&mut self, ctx: CtxId, node: Node) -> NodeId {
        self.states = self.states.saturating_add(1);
        let Some(context) = self.out.contexts.get_mut(ctx) else {
            return 0;
        };
        let id = context.nodes.len();
        context
            .node_index
            .insert((node.block, node.frame, node.globals), id);
        context.nodes.push(node);
        id
    }

    /// The render of a step record and the values it can resume with: `()` for a `Say`,
    /// the value of each shown arm for a `Choice`.
    fn step_outcomes(&self, record: &ConstValue) -> Option<(Option<ConstValue>, Vec<ConstValue>)> {
        let std = self.ctx.std;
        let ConstValue::Adt { item, fields, .. } = record else {
            return None;
        };
        let render_of = |index: Option<usize>| -> Option<ConstValue> {
            let value = fields.get(index?)?;
            std.as_option(value).flatten().cloned()
        };
        if *item == std.say.item {
            return Some((render_of(std.say.field("render")), vec![ConstValue::unit()]));
        }
        if *item != std.choice.item {
            return None;
        }
        let render = render_of(std.choice.field("render"));
        let ConstValue::Array(arms) = fields.get(std.choice.field("arms")?)? else {
            return None;
        };
        let shown = std.choice_arm.field("shown")?;
        let value = std.choice_arm.field("value")?;
        let mut outcomes = Vec::new();
        for arm in arms {
            if arm.field(shown)?.as_bool()? {
                outcomes.push(arm.field(value)?.clone());
            }
        }
        Some((render, outcomes))
    }

    fn fault(&mut self, scene: usize, error: &EvalError, at: Option<ExprId>) {
        let ErrorKind::Fault(fault) = &error.kind else {
            return;
        };
        let Some(data) = self.scenes.list.get(scene) else {
            return;
        };
        let diagnostic = match (at, error.span) {
            (Some(expr), span) => {
                let range = data.b.body.store.expr_range(expr);
                let mut diagnostic = Diagnostic::new(
                    data.item.file.path(self.db).clone(),
                    range,
                    fault.code,
                    fault.message.clone(),
                );
                if let Some(span) = span
                    && (span.file != data.item.file || span.range != range)
                {
                    diagnostic = diagnostic.with_related(
                        span.file.path(self.db).clone(),
                        span.range,
                        "evaluation failed here",
                    );
                }
                diagnostic
            }
            (None, Some(span)) => span.diagnostic(self.db, fault.code, fault.message.clone()),
            (None, None) => return,
        };
        if !self.faults.contains(&diagnostic) {
            self.faults.push(diagnostic);
        }
    }

    fn report_stuck(&self, out: &mut Vec<Diagnostic>) {
        let mut stuck: Vec<_> = self.stuck.iter().collect();
        stuck.sort_by_key(|((scene, expr), _)| (*scene, expr.into_raw().into_u32()));
        for (&(scene, expr), &globals) in stuck {
            let Some(data) = self.scenes.list.get(scene) else {
                continue;
            };
            if !data.game {
                continue;
            }
            let mut message = "this choice can be reached with every arm hidden: the player would \
                               have nothing to select"
                .to_owned();
            let witness = self.witness(scene, expr, globals);
            if !witness.is_empty() {
                message.push_str(&format!(" (for example when {})", witness.join(", ")));
            }
            out.push(data.diagnostic(self.db, expr, DiagnosticCode::AnalysisStuckChoice, message));
        }
    }

    /// The story variables the choice `expr` reads, with their values in a stuck state.
    fn witness(&self, scene: usize, expr: ExprId, globals: u32) -> Vec<String> {
        let (Some(data), Some(values)) =
            (self.scenes.list.get(scene), self.out.globals.get(globals))
        else {
            return Vec::new();
        };
        let store = &data.b.body.store;
        let mut read = Vec::new();
        let mut stack = vec![expr];
        while let Some(expr) = stack.pop() {
            if let Some(ValueRef::Static(item)) = data.b.infer.paths.get(&expr)
                && !read.contains(item)
            {
                read.push(*item);
            }
            if !matches!(store.expr(expr), Expr::Closure { .. }) {
                stack.extend(crate::eval::const_exprs::children(store.expr(expr)));
            }
        }
        read.sort_by_key(|item| (item.file.path(self.db).clone(), item.index));
        read.iter()
            .filter_map(|item| {
                let index = self.out.vars.iter().position(|var| var == item)?;
                let name = item_tree(self.db, item.file)
                    .item(item.index)?
                    .name
                    .clone()?;
                Some(format!(
                    "`{name}` is `{}`",
                    display_value(self.db, values.get(index)?)
                ))
            })
            .collect()
    }
}

impl Explored {
    /// Unreachable scenes and steps, when the exploration was complete.
    pub fn report_reachability(&self, db: &dyn Db, scenes: &Scenes<'_>, out: &mut Vec<Diagnostic>) {
        if !self.complete {
            return;
        }
        let mut reached_blocks: FxHashMap<usize, FxHashSet<BlockId>> = FxHashMap::default();
        for context in &self.contexts {
            let blocks = reached_blocks.entry(context.scene).or_default();
            blocks.extend(context.nodes.iter().map(|node| node.block));
        }
        for (index, scene) in scenes.list.iter().enumerate() {
            if !scene.game {
                continue;
            }
            let Some(blocks) = reached_blocks.get(&index) else {
                out.push(name_diagnostic(
                    db,
                    scene.item,
                    DiagnosticCode::AnalysisUnreachableScene,
                    format!(
                        "the scene `{}` is never reached from an entry scene",
                        scene.name(db)
                    ),
                ));
                continue;
            };
            for (block, data) in scene.body.blocks.iter().enumerate() {
                if let Term::Step { expr, .. } = data.term
                    && !blocks.contains(&block)
                {
                    out.push(scene.diagnostic(
                        db,
                        expr,
                        DiagnosticCode::AnalysisUnreachableStep,
                        "this step is never reached",
                    ));
                }
            }
        }
    }
}

fn name_diagnostic(db: &dyn Db, item: ItemId, code: DiagnosticCode, message: String) -> Diagnostic {
    let range = item_tree(db, item.file)
        .item(item.index)
        .map(|data| data.name_range)
        .unwrap_or_default();
    Diagnostic::new(item.file.path(db).clone(), range, code, message)
}

/// The asset constants of the program by kind: the values of the asset types.
fn assets(db: &dyn Db, ctx: &Ctx<'_>) -> FxHashMap<AssetKind, Vec<ConstValue>> {
    let consts = &const_eval(db, ctx.program).consts;
    let mut out: FxHashMap<AssetKind, Vec<ConstValue>> = FxHashMap::default();
    let mut seen = FxHashSet::default();
    for &(file, _) in &def_maps(db, ctx.program).files {
        for index in 0..item_tree(db, file).items.len() {
            let Ok(index) = u32::try_from(index) else {
                continue;
            };
            if let Some(value @ ConstValue::Asset { kind, .. }) =
                consts.get(&ItemId { file, index })
                && seen.insert(value.clone())
            {
                out.entry(*kind).or_default().push(value.clone());
            }
        }
    }
    out
}

/// The named non-generic functions of the program: the values of function pointer types.
fn functions<'db>(db: &dyn Db, ctx: &Ctx<'db>) -> Vec<(ItemId, &'db FnSig)> {
    let mut out = Vec::new();
    for &(file, _) in &def_maps(db, ctx.program).files {
        for index in 0..item_tree(db, file).items.len() {
            let Ok(index) = u32::try_from(index) else {
                continue;
            };
            let item = ItemId { file, index };
            if let Some(ItemSig::Fn(sig)) = ctx.sigs.get(item)
                && sig.generics.params.is_empty()
                && sig.generics.parent_count == 0
            {
                out.push((item, sig));
            }
        }
    }
    out
}
