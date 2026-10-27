//! The reachable states of the story, from every entry scene and every argument value.
//!
//! Scenes are summarized per *context*: the scene with its arguments and the story
//! variables at entry. A context's states are its suspension points (steps, draws and
//! scene calls) with the kept locals of its frame and the story variables; its exits are
//! the values it can return with the story variables (and the final values of its `&mut`
//! parameters) at return. A call of a context resumes the caller once per exit, as
//! recursive state machines are explored: a scene called from many places with the same
//! entry state is explored once.
//!
//! Values are stored once ([`ValueInterner`]); frames, story variables and contexts refer
//! to them by [`ValueId`]. A segment reads the story variables it uses straight from the
//! interned state and reports only those back, which are `set` into a copy of the state's
//! persistent [`Globals`].

mod model;
mod report;

pub(super) use model::{
    Context, CtxId, ExitData, ExitId, Explored, Frame, FrameId, Globals, GlobalsId, Interner, Node,
    NodeId, NodeKind, Target, ValueId, ValueInterner,
};

use rustc_hash::FxHashMap;

use crate::db::Db;
use crate::diagnostic::{Diag, DiagnosticCode};
use crate::eval::domain::{DomainCtx, product};
use crate::eval::machine::story::StoryValues;
use crate::eval::machine::value::GenericEnv;
use crate::eval::queries::static_value;
use crate::eval::std_items::StdTable;
use crate::eval::{AssetKind, ConstValue, TABLE_DOMAIN_CAP};
use crate::hir::ExprId;
use crate::input::Program;
use crate::item_tree::ItemId;
use crate::mir::{BasicBlock, Terminator};
use crate::signatures::{FnSig, ProgramSignatures};
use crate::ty::{FnDefId, FnRef, GenericArgs};

use super::Scenes;
use super::segment::{Inputs, Resume, Snapshot, Stop, run};
use report::{assets, functions, name_diagnostic};

enum Task {
    Start(CtxId),
    Step {
        ctx: CtxId,
        node: NodeId,
        outcome: usize,
        value: ValueId,
    },
    Return {
        ctx: CtxId,
        node: NodeId,
        exit: ExitId,
    },
}

/// The program-wide inputs of the exploration.
#[derive(Clone, Copy)]
pub(super) struct StoryCx<'db> {
    pub db: &'db dyn Db,
    pub program: Program,
    pub std: &'db StdTable,
    pub sigs: &'db ProgramSignatures,
}

struct Explorer<'a, 'db> {
    cx: StoryCx<'db>,
    scenes: &'a Scenes<'db>,
    out: Explored,
    /// The position of each story variable in [`Explored::vars`].
    slots: FxHashMap<ItemId, usize>,
    ctx_index: FxHashMap<(FnRef, Box<[ValueId]>, GlobalsId), CtxId>,
    tasks: Vec<Task>,
    states: usize,
    /// Choices reached with every arm hidden: scene, step, story variables of a witness.
    stuck: FxHashMap<(usize, ExprId), GlobalsId>,
    faults: Vec<Diag>,
    /// The inputs of the [`DomainCtx`] that enumerates argument values and random draws.
    assets: FxHashMap<AssetKind, Vec<ConstValue>>,
    functions: Vec<(ItemId, &'db FnSig)>,
}

/// How a segment of a context starts.
enum Start {
    Entry,
    After {
        block: BasicBlock,
        frame: FrameId,
        value: ConstValue,
        outs: Vec<ConstValue>,
    },
}

/// Explores the story from every entry scene; `None` when there is nothing to explore.
pub(super) fn explore(
    cx: StoryCx<'_>,
    scenes: &Scenes<'_>,
    vars: &[ItemId],
    diagnostics: &mut Vec<Diag>,
) -> Option<Explored> {
    let entries: Vec<usize> = scenes
        .list
        .iter()
        .enumerate()
        .filter(|(_, scene)| {
            scene.game
                && cx.sigs.fn_sig(scene.item).is_some_and(|sig| {
                    sig.entry && sig.generics.params.is_empty() && sig.generics.parent_count == 0
                })
        })
        .map(|(index, _)| index)
        .collect();
    if entries.is_empty() {
        return None;
    }
    let mut out = Explored {
        contexts: Vec::new(),
        roots: Vec::new(),
        values: ValueInterner(Interner::new()),
        frames: Interner::new(),
        globals: Interner::new(),
        vars: vars.to_vec(),
        complete: true,
    };
    let mut initial = Globals::new();
    for var in vars {
        // Errors in initializers are stage 4's.
        let value = static_value(cx.db, cx.program, *var).as_ref().ok()?;
        initial.push_back(out.values.intern(value.value.clone()));
    }
    let globals = GlobalsId(out.globals.intern(initial));
    let mut explorer = Explorer {
        cx,
        scenes,
        slots: vars
            .iter()
            .enumerate()
            .map(|(slot, var)| (*var, slot))
            .collect(),
        out,
        ctx_index: FxHashMap::default(),
        tasks: Vec::new(),
        states: 0,
        stuck: FxHashMap::default(),
        faults: Vec::new(),
        assets: assets(cx),
        functions: functions(cx),
    };
    let mut roots = Vec::new();
    for &scene in &entries {
        let Some(data) = scenes.list.get(scene) else {
            continue;
        };
        let params = cx
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
                data.item,
                DiagnosticCode::AnalysisTooManyStates,
                format!(
                    "the entry scene `{}` takes too many argument values to analyze: more than \
                     the limit of {TABLE_DOMAIN_CAP}, so unreachable steps and scenes are not \
                     reported",
                    data.name(cx.db)
                ),
            ));
            continue;
        }
        let fn_ref = FnRef {
            def: FnDefId::Item(data.item),
            self_ty: None,
            args: GenericArgs::empty(),
        };
        for args in product(&domains) {
            roots.push((scene, fn_ref.clone(), args));
        }
    }
    for (scene, fn_ref, args) in roots {
        let args = explorer.intern_values(args);
        let root = explorer.context(scene, fn_ref, args, globals);
        explorer.out.roots.push(root);
    }
    explorer.run(diagnostics, &entries);
    diagnostics.append(&mut explorer.faults);
    explorer.report_stuck(diagnostics);
    Some(explorer.out)
}

impl Explorer<'_, '_> {
    fn domain(&self) -> DomainCtx<'_> {
        DomainCtx {
            sigs: self.cx.sigs,
            std: self.cx.std,
            texts: &[],
            assets: &self.assets,
            functions: &self.functions,
        }
    }

    fn intern_values(&mut self, values: Vec<ConstValue>) -> Box<[ValueId]> {
        values
            .into_iter()
            .map(|value| self.out.values.intern(value))
            .collect()
    }

    fn values(&self, ids: &[ValueId]) -> Vec<ConstValue> {
        ids.iter()
            .map(|id| {
                self.out
                    .values
                    .get(*id)
                    .cloned()
                    .unwrap_or_else(ConstValue::unit)
            })
            .collect()
    }

    fn value(&self, id: ValueId) -> ConstValue {
        self.out
            .values
            .get(id)
            .cloned()
            .unwrap_or_else(ConstValue::unit)
    }

    fn ctx_mut(&mut self, id: CtxId) -> Option<&mut Context> {
        self.out.contexts.get_mut(id.idx())
    }

    /// The context of `scene` entered with `args` and `globals`, explored once.
    fn context(
        &mut self,
        scene: usize,
        fn_ref: FnRef,
        args: Box<[ValueId]>,
        globals: GlobalsId,
    ) -> CtxId {
        let key = (fn_ref, args, globals);
        if let Some(id) = self.ctx_index.get(&key) {
            return *id;
        }
        let id = CtxId::from_usize(self.out.contexts.len());
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

    fn run(&mut self, diagnostics: &mut Vec<Diag>, entries: &[usize]) {
        let cap = self.cx.program.options(self.cx.db).state_cap;
        let limit = usize::try_from(cap).unwrap_or(usize::MAX);
        while let Some(task) = self.tasks.pop() {
            if self.states > limit {
                self.out.complete = false;
                if let Some(data) = entries
                    .first()
                    .and_then(|scene| self.scenes.list.get(*scene))
                {
                    diagnostics.push(name_diagnostic(
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
                    let Some(globals) = self.out.context(id).map(|context| context.globals) else {
                        continue;
                    };
                    let target = self.resume(id, Start::Entry, globals);
                    if let Some(context) = self.ctx_mut(id) {
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
                    let value = self.value(value);
                    let start = Start::After {
                        block,
                        frame,
                        value,
                        outs: Vec::new(),
                    };
                    let target = self.resume(ctx, start, globals);
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
                    let Some(callee) = self.callee(ctx, node) else {
                        continue;
                    };
                    let Some((ret, globals, outs)) = self
                        .out
                        .context(callee)
                        .and_then(|callee| callee.exits.get(exit.idx()))
                        .map(|data| (data.ret, data.globals, data.outs.clone()))
                    else {
                        continue;
                    };
                    let start = Start::After {
                        block,
                        frame,
                        value: self.value(ret),
                        outs: self.values(&outs),
                    };
                    let target = self.resume(ctx, start, globals);
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

    fn node_state(&self, ctx: CtxId, node: NodeId) -> Option<(BasicBlock, FrameId, GlobalsId)> {
        let node = self.out.context(ctx)?.node(node)?;
        Some((node.block, node.frame, node.globals))
    }

    fn node_mut(&mut self, ctx: CtxId, node: NodeId) -> Option<&mut Node> {
        self.ctx_mut(ctx)?.nodes.get_mut(node.idx())
    }

    fn callee(&self, ctx: CtxId, node: NodeId) -> Option<CtxId> {
        match self.out.context(ctx)?.node(node)?.kind {
            NodeKind::Call { callee, .. } => Some(callee),
            NodeKind::Step { .. } | NodeKind::Rng { .. } => None,
        }
    }

    /// The source of the terminator of `block` in context `ctx`'s scene.
    fn term_src(&self, ctx: CtxId, block: BasicBlock) -> Option<ExprId> {
        let scene = self.out.context(ctx)?.scene;
        self.scenes.list.get(scene)?.mir.main.block(block)?.term_src
    }

    fn is_term(&self, ctx: CtxId, block: BasicBlock, test: fn(&Terminator) -> bool) -> bool {
        let Some(scene) = self.out.context(ctx).map(|context| context.scene) else {
            return false;
        };
        self.scenes
            .list
            .get(scene)
            .and_then(|scene| scene.mir.main.block(block))
            .is_some_and(|data| test(&data.term))
    }

    /// Runs context `ctx` from `start` to its next suspension and records where it got.
    fn resume(&mut self, ctx: CtxId, start: Start, globals: GlobalsId) -> Target {
        let Some(context) = self.out.context(ctx) else {
            return Target::Dead;
        };
        let Some(data) = self.scenes.list.get(context.scene) else {
            return Target::Dead;
        };
        let Some(vars) = self.out.globals.get(globals.0) else {
            return Target::Dead;
        };
        let vars = StateGlobals {
            values: &self.out.values,
            globals: vars,
        };
        let env = GenericEnv::for_item(self.cx.sigs, data.item, context.fn_ref.args.clone());
        let inputs = Inputs {
            db: self.cx.db,
            program: self.cx.program,
            std: self.cx.std,
            slots: &self.slots,
        };
        let (stop, snapshot) = match &start {
            Start::Entry => {
                let args = self.values(&context.args);
                run(inputs, data.body(), env, Resume::Entry(&args), &vars)
            }
            Start::After {
                block,
                frame,
                value,
                outs,
            } => {
                let Some(frame) = self.out.frames.get(frame.0) else {
                    return Target::Dead;
                };
                let frame: Vec<Option<&ConstValue>> = frame
                    .iter()
                    .map(|id| id.and_then(|id| self.out.values.get(id)))
                    .collect();
                let resume = Resume::After {
                    block: *block,
                    frame: &frame,
                    value,
                    outs,
                };
                run(inputs, data.body(), env, resume, &vars)
            }
        };
        self.settle(ctx, stop, snapshot, globals)
    }

    /// Interns the frame and story variables of a snapshot taken from a segment that started
    /// with the story variables `globals`; returns the node of that state if it exists.
    fn state(
        &mut self,
        ctx: CtxId,
        block: BasicBlock,
        snapshot: Snapshot,
        globals: GlobalsId,
    ) -> (FrameId, GlobalsId, Option<NodeId>) {
        let frame: Frame = snapshot
            .frame
            .into_iter()
            .map(|value| value.map(|value| self.out.values.intern(value)))
            .collect();
        let frame = FrameId(self.out.frames.intern(frame));
        let globals = self.update_globals(globals, snapshot.globals);
        let node = self
            .out
            .context(ctx)
            .and_then(|context| context.node_index.get(&(block, frame, globals)))
            .copied();
        (frame, globals, node)
    }

    /// The story variables `base` with the accessed ones set to their new values.
    fn update_globals(&mut self, base: GlobalsId, accessed: Vec<(usize, ConstValue)>) -> GlobalsId {
        let Some(mut globals) = self.out.globals.get(base.0).cloned() else {
            return base;
        };
        let mut changed = false;
        for (slot, value) in accessed {
            let id = self.out.values.intern(value);
            if let Some(old) = globals.get_mut(slot)
                && *old != id
            {
                *old = id;
                changed = true;
            }
        }
        if !changed {
            return base;
        }
        GlobalsId(self.out.globals.intern(globals))
    }

    fn settle(&mut self, ctx: CtxId, stop: Stop, snapshot: Snapshot, start: GlobalsId) -> Target {
        let Some(scene_index) = self.out.context(ctx).map(|context| context.scene) else {
            return Target::Dead;
        };
        let last = snapshot.last;
        match stop {
            Stop::Step { block, record } => {
                let (frame, globals, node) = self.state(ctx, block, snapshot, start);
                if let Some(node) = node {
                    return Target::Node(node);
                }
                let Some(expr) = self.term_src(ctx, block) else {
                    return Target::Dead;
                };
                let Some((render, outcomes)) = self.step_outcomes(&record) else {
                    self.give_up(scene_index, Some(expr));
                    return Target::Dead;
                };
                if outcomes.is_empty() {
                    self.stuck.entry((scene_index, expr)).or_insert(globals);
                }
                let choice = matches!(&record, ConstValue::Adt { item, .. } if *item == self.cx.std.choice.item);
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
                    let value = self.out.values.intern(value);
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
                let (frame, globals, node) = self.state(ctx, block, snapshot, start);
                if let Some(node) = node {
                    return Target::Node(node);
                }
                if !self.is_term(ctx, block, |term| matches!(term, Terminator::Rng { .. })) {
                    return Target::Dead;
                }
                let Some(values) = self.domain().values(&ty, TABLE_DOMAIN_CAP) else {
                    self.too_many_draws(scene_index, self.term_src(ctx, block));
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
                    let value = self.out.values.intern(value);
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
                let (frame, globals, node) = self.state(ctx, block, snapshot, start);
                if let Some(node) = node {
                    return Target::Node(node);
                }
                let scene = match callee.def {
                    FnDefId::Item(item) => self.scenes.index.get(&item).copied(),
                    _ => None,
                };
                let Some(scene) = scene else {
                    self.give_up(scene_index, last);
                    return Target::Dead;
                };
                let args = self.intern_values(args);
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
                let exits = match self.ctx_mut(callee_ctx) {
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
                        exit: ExitId::from_usize(exit),
                    });
                }
                Target::Node(id)
            }
            Stop::Return { value, outs } => {
                let globals = self.update_globals(start, snapshot.globals);
                let data = ExitData {
                    ret: self.out.values.intern(value),
                    globals,
                    outs: self.intern_values(outs),
                };
                let Some(context) = self.out.contexts.get_mut(ctx.idx()) else {
                    return Target::Dead;
                };
                if let Some(exit) = context.exit_index.get(&data) {
                    return Target::Exit(*exit);
                }
                let exit = ExitId::from_usize(context.exits.len());
                context.exits.push(data.clone());
                context.exit_index.insert(data, exit);
                for &(caller, node) in &context.callers {
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
                self.give_up(scene_index, last);
                Target::Dead
            }
        }
    }

    fn add_node(&mut self, ctx: CtxId, node: Node) -> NodeId {
        self.states = self.states.saturating_add(1);
        let Some(context) = self.ctx_mut(ctx) else {
            return NodeId(0);
        };
        let id = NodeId::from_usize(context.nodes.len());
        context
            .node_index
            .insert((node.block, node.frame, node.globals), id);
        context.nodes.push(node);
        id
    }

    /// The render of a step record and the values it can resume with: `()` for a `Say`,
    /// the value of each shown arm for a `Choice`.
    fn step_outcomes(&self, record: &ConstValue) -> Option<(Option<ConstValue>, Vec<ConstValue>)> {
        let std = self.cx.std;
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
}

/// The story variables of a state, as a segment reads them.
struct StateGlobals<'a> {
    values: &'a ValueInterner,
    globals: &'a Globals,
}

impl StoryValues for StateGlobals<'_> {
    fn value(&self, slot: usize) -> Option<&ConstValue> {
        self.values.get(*self.globals.get(slot)?)
    }
}
