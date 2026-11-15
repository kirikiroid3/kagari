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
//! States are shared, never merged (`SceneShape`). A scene's context holds only the story
//! variables the scene can access (its footprint): the others are [`ValueId::ABSENT`] in
//! it, and keep the values they have at the call node, which the caller puts back around
//! the callee's on return. So a scene is explored once per valuation of what it can see.
//!
//! Values are stored once ([`ValueInterner`]); frames, story variables and contexts refer
//! to them by [`ValueId`]. A segment reads the story variables it uses straight from the
//! interned state and reports only those back, which are `set` into a copy of the state's
//! persistent [`Globals`].

mod model;
mod report;

pub(super) use model::{
    Context, CtxId, ExitData, ExitId, Explored, Frame, FrameId, Globals, GlobalsId, Interner, Node,
    NodeId, NodeKind, RegionRef, Target, ValueId, ValueInterner,
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
use super::segment::{Bounds, Inputs, Resume, Scratch, Snapshot, Stop, run};
use super::shape::{SceneShape, SlotSet};
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

/// What a context is explored once for.
#[derive(Clone, PartialEq, Eq, Hash)]
struct CtxKey {
    fn_ref: FnRef,
    args: Box<[ValueId]>,
    region: Option<RegionRef>,
    /// The story variables the context holds, at entry.
    globals: GlobalsId,
}

struct Explorer<'a, 'db> {
    cx: StoryCx<'db>,
    scenes: &'a Scenes<'db>,
    /// The shape of every scene, by `Scenes::list` index.
    shapes: &'a [SceneShape],
    /// The machine vectors the segments reuse.
    scratch: Scratch,
    out: Explored,
    /// The position of each story variable in [`Explored::vars`].
    slots: FxHashMap<ItemId, usize>,
    /// The story variables with none held.
    absent: GlobalsId,
    ctx_index: FxHashMap<CtxKey, CtxId>,
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
    /// At the suspension ending `block`, with its kept locals: the header of a region, or
    /// where a region was left.
    At {
        block: BasicBlock,
        frame: FrameId,
    },
}

/// Explores the story from every entry scene; `None` when there is nothing to explore.
pub(super) fn explore(
    cx: StoryCx<'_>,
    scenes: &Scenes<'_>,
    shapes: &[SceneShape],
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
    let absent = GlobalsId(
        out.globals
            .intern(vars.iter().map(|_| ValueId::ABSENT).collect()),
    );
    let mut explorer = Explorer {
        cx,
        scenes,
        shapes,
        scratch: Scratch::default(),
        slots: vars
            .iter()
            .enumerate()
            .map(|(slot, var)| (*var, slot))
            .collect(),
        out,
        absent,
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
        let root = explorer.scene_context(scene, fn_ref, args, globals);
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

    /// The context with these key parts, explored once.
    fn context(
        &mut self,
        scene: usize,
        fn_ref: FnRef,
        args: Box<[ValueId]>,
        region: Option<RegionRef>,
        globals: GlobalsId,
    ) -> CtxId {
        let key = CtxKey {
            fn_ref,
            args,
            region,
            globals,
        };
        if let Some(id) = self.ctx_index.get(&key) {
            return *id;
        }
        let id = CtxId::from_usize(self.out.contexts.len());
        self.out.contexts.push(Context {
            scene,
            fn_ref: key.fn_ref.clone(),
            args: key.args.clone(),
            region,
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

    /// The context of `scene` entered with `args` and, of the story variables `globals`,
    /// those the scene can access.
    fn scene_context(
        &mut self,
        scene: usize,
        fn_ref: FnRef,
        args: Box<[ValueId]>,
        globals: GlobalsId,
    ) -> CtxId {
        let shapes = self.shapes;
        let globals = match shapes.get(scene).and_then(|shape| shape.footprint.as_ref()) {
            Some(footprint) => self.project(globals, footprint),
            None => globals,
        };
        self.context(scene, fn_ref, args, None, globals)
    }

    /// `globals` with the story variables outside `keep` absent.
    fn project(&mut self, globals: GlobalsId, keep: &SlotSet) -> GlobalsId {
        if keep.is_empty() {
            return self.absent;
        }
        let Some(values) = self.out.globals.get(globals.0) else {
            return globals;
        };
        let projected: Globals = values
            .iter()
            .enumerate()
            .map(|(slot, value)| {
                if keep.contains(slot) {
                    *value
                } else {
                    ValueId::ABSENT
                }
            })
            .collect();
        if projected == *values {
            return globals;
        }
        GlobalsId(self.out.globals.intern(projected))
    }

    /// `base` with the story variables `over` holds set to their values in `over`.
    fn overlay(&mut self, base: GlobalsId, over: GlobalsId) -> GlobalsId {
        let (Some(values), Some(over)) =
            (self.out.globals.get(base.0), self.out.globals.get(over.0))
        else {
            return base;
        };
        let mut merged = values.clone();
        let mut changed = false;
        for (slot, value) in over.iter().enumerate() {
            if *value != ValueId::ABSENT && merged.get(slot) != Some(value) {
                merged.set(slot, *value);
                changed = true;
            }
        }
        if !changed {
            return base;
        }
        GlobalsId(self.out.globals.intern(merged))
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
                    let Some((region, globals)) = self
                        .out
                        .context(id)
                        .map(|context| (context.region, context.globals))
                    else {
                        continue;
                    };
                    let start = match region {
                        Some(region) => Start::At {
                            block: region.header,
                            frame: region.frame,
                        },
                        None => Start::Entry,
                    };
                    let target = self.resume(id, start, globals);
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
                    let Some((block, frame, globals)) = self.node_state(ctx, node) else {
                        continue;
                    };
                    let Some(callee) = self.callee(ctx, node) else {
                        continue;
                    };
                    let Some((entry, data)) = self.out.context(callee).and_then(|callee| {
                        Some((callee.globals, callee.exits.get(exit.idx())?.clone()))
                    }) else {
                        continue;
                    };
                    let target = match data {
                        ExitData::Return {
                            ret,
                            globals: returned,
                            outs,
                        } => {
                            // The callee holds the story variables it can access; the others
                            // still have the values they have at the call.
                            let globals = if returned == entry {
                                globals
                            } else {
                                self.overlay(globals, returned)
                            };
                            let start = Start::After {
                                block,
                                frame,
                                value: self.value(ret),
                                outs: self.values(&outs),
                            };
                            self.resume(ctx, start, globals)
                        }
                        // The region changed no story variable: the caller goes on where
                        // the region stopped, with its own state around the region's.
                        ExitData::Leave {
                            block: to,
                            frame: held,
                        } => match self.rejoin(ctx, block, frame, to, held) {
                            Some(frame) => {
                                self.resume(ctx, Start::At { block: to, frame }, globals)
                            }
                            None => {
                                let at = self.term_src(ctx, to);
                                if let Some(scene) = self.out.context(ctx).map(|c| c.scene) {
                                    self.give_up(scene, at);
                                }
                                Target::Dead
                            }
                        },
                    };
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
        let (Some(data), Some(shape)) = (
            self.scenes.list.get(context.scene),
            self.shapes.get(context.scene),
        ) else {
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
        let (at, frame) = match &start {
            Start::Entry => (None, None),
            Start::After { block, frame, .. } | Start::At { block, frame } => {
                (Some(*block), Some(*frame))
            }
        };
        let bounds = Bounds {
            touched: shape.touched(at),
            by_ref: &shape.by_ref,
            region: context
                .region
                .and_then(|region| shape.regions.get(region.region as usize))
                .map(|region| &region.blocks),
        };
        let frame: Vec<Option<&ConstValue>> = match frame {
            Some(frame) => {
                let Some(frame) = self.out.frames.get(frame.0) else {
                    return Target::Dead;
                };
                frame
                    .iter()
                    .map(|id| id.and_then(|id| self.out.values.get(id)))
                    .collect()
            }
            None => Vec::new(),
        };
        let args;
        let resume = match &start {
            Start::Entry => {
                args = self.values(&context.args);
                Resume::Entry(&args)
            }
            Start::After {
                block, value, outs, ..
            } => Resume::After {
                block: *block,
                frame: &frame,
                value,
                outs,
            },
            Start::At { block, .. } => Resume::At {
                block: *block,
                frame: &frame,
            },
        };
        let (stop, snapshot) = run(
            inputs,
            data.body(),
            env,
            resume,
            &vars,
            bounds,
            &mut self.scratch,
        );
        self.settle(ctx, stop, snapshot, globals)
    }

    fn intern_frame(&mut self, frame: Vec<Option<ConstValue>>) -> FrameId {
        let frame: Frame = frame
            .into_iter()
            .map(|value| value.map(|value| self.out.values.intern(value)))
            .collect();
        FrameId(self.out.frames.intern(frame))
    }

    /// Interns the frame of a snapshot; returns the node of the state it makes with `block`
    /// and the story variables `globals`, if it exists.
    fn state(
        &mut self,
        ctx: CtxId,
        block: BasicBlock,
        frame: Vec<Option<ConstValue>>,
        globals: GlobalsId,
    ) -> (FrameId, Option<NodeId>) {
        let frame = self.intern_frame(frame);
        let node = self
            .out
            .context(ctx)
            .and_then(|context| context.node_index.get(&(block, frame, globals)))
            .copied();
        (frame, node)
    }

    /// When a region starts at `block` for `ctx`: the call node of `ctx`, with the state
    /// `frame` and `globals`, that enters the region's context for the part of that state
    /// the region reads. The region's context creates the node of the suspension itself.
    fn enter_region(
        &mut self,
        ctx: CtxId,
        block: BasicBlock,
        frame: FrameId,
        globals: GlobalsId,
    ) -> Option<Target> {
        let shapes = self.shapes;
        let context = self.out.context(ctx)?;
        let shape = shapes.get(context.scene)?;
        let index = shape.region_at(context.region.map(|region| region.region), block)?;
        let region = shape.regions.get(index as usize)?;
        let (scene, fn_ref) = (context.scene, context.fn_ref.clone());
        let key: Frame = self
            .out
            .frames
            .get(frame.0)?
            .iter()
            .zip(region.key_locals.iter())
            .map(|(value, read)| value.filter(|_| *read))
            .collect();
        let key = RegionRef {
            region: index,
            header: region.header,
            frame: FrameId(self.out.frames.intern(key)),
        };
        let held = self.project(globals, &region.key_vars);
        let callee = self.context(scene, fn_ref, Box::default(), Some(key), held);
        Some(self.call(ctx, block, frame, globals, callee))
    }

    /// The kept locals of `to`, where a region entered at the call node of `ctx` at `block`
    /// with the kept locals `frame` was left holding `held`: what the region holds, around
    /// it what the call node holds.
    fn rejoin(
        &mut self,
        ctx: CtxId,
        block: BasicBlock,
        frame: FrameId,
        to: BasicBlock,
        held: FrameId,
    ) -> Option<FrameId> {
        let scene = self.scenes.list.get(self.out.context(ctx)?.scene)?;
        let raw = |local: crate::mir::Local| local.into_raw().into_u32();
        let mut own = scene
            .kept
            .get(&block)?
            .iter()
            .zip(self.out.frames.get(frame.0)?.iter())
            .peekable();
        let joined: Frame = scene
            .kept
            .get(&to)?
            .iter()
            .zip(self.out.frames.get(held.0)?.iter())
            .map(|(local, value)| {
                while own.next_if(|(kept, _)| raw(*kept) < raw(local)).is_some() {}
                let outer = own
                    .peek()
                    .filter(|(kept, _)| *kept == local)
                    .and_then(|(_, value)| **value);
                value.or(outer)
            })
            .collect();
        Some(FrameId(self.out.frames.intern(joined)))
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
        let Some((scene_index, region)) = self
            .out
            .context(ctx)
            .map(|context| (context.scene, context.region))
        else {
            return Target::Dead;
        };
        let last = snapshot.last;
        // The story variables after the segment.
        let globals = match stop {
            Stop::Fault { .. } | Stop::Abort => start,
            _ => self.update_globals(start, snapshot.globals),
        };
        // The segments of a region write no story variable, call no scene and never return:
        // a region whose segment does is not what its shape says.
        if region.is_some()
            && (globals != start || matches!(stop, Stop::Return { .. } | Stop::Call { .. }))
        {
            self.give_up(scene_index, last);
            return Target::Dead;
        }
        match stop {
            Stop::Step { block, record } => {
                let (frame, node) = self.state(ctx, block, snapshot.frame, globals);
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
                    // Before a region is entered: its context need not hold the story
                    // variables the arms' conditions read.
                    self.stuck.entry((scene_index, expr)).or_insert(globals);
                }
                if let Some(target) = self.enter_region(ctx, block, frame, globals) {
                    return target;
                }
                let choice = matches!(&record, ConstValue::Adt { item, .. } if *item == self.cx.std.choice.item);
                let outcomes: Box<[(u32, ValueId)]> = outcomes
                    .into_iter()
                    .map(|(arm, value)| (arm, self.out.values.intern(value)))
                    .collect();
                let tasks = outcomes.len();
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
                            outcomes: outcomes.clone(),
                            succ: vec![Target::Dead; tasks],
                        },
                    },
                );
                for (outcome, &(_, value)) in outcomes.iter().enumerate() {
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
                let (frame, node) = self.state(ctx, block, snapshot.frame, globals);
                if let Some(node) = node {
                    return Target::Node(node);
                }
                if let Some(target) = self.enter_region(ctx, block, frame, globals) {
                    return target;
                }
                if !self.is_term(ctx, block, |term| matches!(term, Terminator::Rng { .. })) {
                    return Target::Dead;
                }
                let Some(values) = self.domain().values(&ty, TABLE_DOMAIN_CAP) else {
                    self.too_many_draws(scene_index, self.term_src(ctx, block));
                    return Target::Dead;
                };
                let draws: Box<[ValueId]> = values
                    .into_iter()
                    .map(|value| self.out.values.intern(value))
                    .collect();
                let id = self.add_node(
                    ctx,
                    Node {
                        block,
                        frame,
                        globals,
                        kind: NodeKind::Rng {
                            draws: draws.clone(),
                            succ: vec![Target::Dead; draws.len()],
                        },
                    },
                );
                for (outcome, &value) in draws.iter().enumerate() {
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
                let (frame, node) = self.state(ctx, block, snapshot.frame, globals);
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
                let callee = self.scene_context(scene, callee, args, globals);
                self.call(ctx, block, frame, globals, callee)
            }
            Stop::Return { value, outs } => {
                let data = ExitData::Return {
                    ret: self.out.values.intern(value),
                    globals,
                    outs: self.intern_values(outs),
                };
                self.exit(ctx, data)
            }
            Stop::Leave { block } => {
                let frame = self.intern_frame(snapshot.frame);
                self.exit(ctx, ExitData::Leave { block, frame })
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

    /// A call node of `ctx` at `block` that enters `callee`; it resumes once per exit of
    /// the callee, those it already has included.
    fn call(
        &mut self,
        ctx: CtxId,
        block: BasicBlock,
        frame: FrameId,
        globals: GlobalsId,
        callee: CtxId,
    ) -> Target {
        let id = self.add_node(
            ctx,
            Node {
                block,
                frame,
                globals,
                kind: NodeKind::Call {
                    callee,
                    succ: Vec::new(),
                },
            },
        );
        let exits = match self.ctx_mut(callee) {
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

    /// The exit `data` of `ctx`, added once; a new exit resumes every caller the context
    /// has so far.
    fn exit(&mut self, ctx: CtxId, data: ExitData) -> Target {
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

    /// The render of a step record and the values it can resume with, each with its arm's
    /// position in the arm list: `(0, ())` for a `Say`, each shown arm for a `Choice`.
    #[allow(clippy::type_complexity)]
    fn step_outcomes(
        &self,
        record: &ConstValue,
    ) -> Option<(Option<ConstValue>, Vec<(u32, ConstValue)>)> {
        let std = self.cx.std;
        let ConstValue::Adt { item, fields, .. } = record else {
            return None;
        };
        let render_of = |index: Option<usize>| -> Option<ConstValue> {
            let value = fields.get(index?)?;
            std.as_option(value).flatten().cloned()
        };
        if *item == std.say.item {
            return Some((
                render_of(std.say.field("render")),
                vec![(0, ConstValue::unit())],
            ));
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
        for (index, arm) in arms.iter().enumerate() {
            if arm.field(shown)?.as_bool()? {
                let index = u32::try_from(index).ok()?;
                outcomes.push((index, arm.field(value)?.clone()));
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
