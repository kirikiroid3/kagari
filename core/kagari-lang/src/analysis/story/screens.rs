//! The screens that reach each step (`docs/syntax/src/presentation.md#consistency` and
//! `#unbounded-screens`).
//!
//! Renders never influence the story, so screens are propagated over the explored states
//! afterwards. Each context is instantiated per entry screen; a step applies its render to
//! every screen that reaches it, a call enters the callee's instance for that screen and
//! resumes with the screens of its exits. The story starts on `Screen::empty()`.
//!
//! - **Bounded screens.** When the set of screens reaching a step grows past
//!   [`SCREEN_CAP`], the render that produced the new screen is reported and left out (it
//!   leaves the screen unchanged), and the propagation starts over.
//! - **Consistency.** The screen a step shows must be a function of the story state: of
//!   the step, the live locals of every frame on the call stack, the story variables and
//!   the call sites. Contexts and states already hold everything but the call stack, so
//!   the check walks the call tree grouping the instances of a context entered from equal
//!   call sites: a step whose state is reached with two different screens in one group is
//!   reported.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::db::Db;
use crate::diagnostic::{Diagnostic, DiagnosticCode, line_col};
use crate::eval::ConstValue;
use crate::eval::interp::{Ctx, ErrorKind, Frame, Machine, Value};
use crate::eval::screen::{LogicalScreen, Screen, duration_overflow};
use crate::hir::{Expr, ExprId};
use crate::infer::ValueRef;
use crate::ty::{FnDefId, FnRef};

use super::explore::{CtxId, Explored, Interner, NodeId, NodeKind, Target};
use super::{SCREEN_CAP, Scenes};

type ScreenId = u32;
/// A step: its scene (index into `Scenes::list`) and its `push_step` call.
type StepRef = (usize, ExprId);
type InstId = usize;
/// The step whose render last changed a screen; `None` for the screen the story starts on.
type Producer = Option<StepRef>;

/// A context entered with one screen.
struct Inst {
    ctx: CtxId,
    node_in: Vec<FxHashMap<ScreenId, Producer>>,
    exit_out: Vec<FxHashMap<ScreenId, Producer>>,
    callers: Vec<(InstId, NodeId)>,
}

/// A step whose set of screens grew past the cap, and the render that grew it.
struct Overflow {
    step: StepRef,
    producer: Producer,
}

struct Screens<'a, 'c, 'db> {
    db: &'db dyn Db,
    ctx: &'c Ctx<'db>,
    scenes: &'a Scenes<'db>,
    explored: &'a Explored,
    screens: Interner<LogicalScreen>,
    start: ScreenId,
    insts: Vec<Inst>,
    inst_index: FxHashMap<(CtxId, ScreenId), InstId>,
    renders: FxHashMap<(ConstValue, ScreenId), ScreenId>,
    step_sets: FxHashMap<StepRef, FxHashSet<ScreenId>>,
    disabled: FxHashSet<StepRef>,
    work: Vec<(InstId, Target, ScreenId, Producer)>,
    diagnostics: Vec<Diagnostic>,
}

pub(super) fn check(
    db: &dyn Db,
    ctx: &Ctx<'_>,
    scenes: &Scenes<'_>,
    explored: &Explored,
    out: &mut Vec<Diagnostic>,
) {
    let mut screens = Interner::new();
    let start = screens.intern(Screen::empty(ctx.std).logical);
    let mut state = Screens {
        db,
        ctx,
        scenes,
        explored,
        screens,
        start,
        insts: Vec::new(),
        inst_index: FxHashMap::default(),
        renders: FxHashMap::default(),
        step_sets: FxHashMap::default(),
        disabled: FxHashSet::default(),
        work: Vec::new(),
        diagnostics: Vec::new(),
    };
    loop {
        match state.propagate() {
            Ok(()) => break,
            Err(overflow) => {
                let culprit = overflow.producer.unwrap_or(overflow.step);
                if !state.disabled.insert(culprit) {
                    break;
                }
                state.report_overflow(culprit, overflow.step);
            }
        }
    }
    // A screen difference found on reachable states is real even when the exploration
    // stopped early.
    state.consistency();
    out.append(&mut state.diagnostics);
}

impl Screens<'_, '_, '_> {
    fn node_count(&self, ctx: CtxId) -> usize {
        self.explored.contexts.get(ctx).map_or(0, |c| c.nodes.len())
    }

    fn exit_count(&self, ctx: CtxId) -> usize {
        self.explored.contexts.get(ctx).map_or(0, |c| c.exits.len())
    }

    /// The instance of `ctx` entered with `screen`, created (and its entry scheduled) once.
    fn inst(&mut self, ctx: CtxId, screen: ScreenId, producer: Producer) -> InstId {
        if let Some(id) = self.inst_index.get(&(ctx, screen)) {
            return *id;
        }
        let id = self.insts.len();
        self.insts.push(Inst {
            ctx,
            node_in: vec![FxHashMap::default(); self.node_count(ctx)],
            exit_out: vec![FxHashMap::default(); self.exit_count(ctx)],
            callers: Vec::new(),
        });
        self.inst_index.insert((ctx, screen), id);
        if let Some(context) = self.explored.contexts.get(ctx) {
            self.work.push((id, context.entry, screen, producer));
        }
        id
    }

    fn propagate(&mut self) -> Result<(), Overflow> {
        self.insts.clear();
        self.inst_index.clear();
        self.step_sets.clear();
        self.work.clear();
        for root in self.explored.roots.clone() {
            self.inst(root, self.start, None);
        }
        while let Some((inst, target, screen, producer)) = self.work.pop() {
            let Some(ctx) = self.insts.get(inst).map(|inst| inst.ctx) else {
                continue;
            };
            match target {
                Target::Node(node) => {
                    let fresh = match self
                        .insts
                        .get_mut(inst)
                        .and_then(|inst| inst.node_in.get_mut(node))
                    {
                        Some(set) if !set.contains_key(&screen) => {
                            set.insert(screen, producer);
                            true
                        }
                        _ => false,
                    };
                    if !fresh {
                        continue;
                    }
                    let Some(context) = self.explored.contexts.get(ctx) else {
                        continue;
                    };
                    let Some(data) = context.nodes.get(node) else {
                        continue;
                    };
                    match &data.kind {
                        NodeKind::Step { expr, render, succ } => {
                            let step = (context.scene, *expr);
                            let set = self.step_sets.entry(step).or_default();
                            set.insert(screen);
                            if set.len() > SCREEN_CAP {
                                return Err(Overflow { step, producer });
                            }
                            let shown = self.apply(step, render.as_ref(), screen);
                            let producer = if shown == screen {
                                producer
                            } else {
                                Some(step)
                            };
                            for target in succ {
                                self.work.push((inst, *target, shown, producer));
                            }
                        }
                        NodeKind::Call { callee, succ } => {
                            let callee_inst = self.inst(*callee, screen, producer);
                            let mut pending = Vec::new();
                            if let Some(callee_data) = self.insts.get_mut(callee_inst) {
                                callee_data.callers.push((inst, node));
                                for (exit, target) in succ {
                                    if let Some(screens) = callee_data.exit_out.get(*exit) {
                                        pending
                                            .extend(screens.iter().map(|(s, p)| (*target, *s, *p)));
                                    }
                                }
                            }
                            for (target, screen, producer) in pending {
                                self.work.push((inst, target, screen, producer));
                            }
                        }
                    }
                }
                Target::Exit(exit) => {
                    let fresh = match self
                        .insts
                        .get_mut(inst)
                        .and_then(|inst| inst.exit_out.get_mut(exit))
                    {
                        Some(set) if !set.contains_key(&screen) => {
                            set.insert(screen, producer);
                            true
                        }
                        _ => false,
                    };
                    if !fresh {
                        continue;
                    }
                    let callers = self
                        .insts
                        .get(inst)
                        .map(|inst| inst.callers.clone())
                        .unwrap_or_default();
                    for (caller, node) in callers {
                        let Some(caller_ctx) = self.insts.get(caller).map(|inst| inst.ctx) else {
                            continue;
                        };
                        let target = self
                            .explored
                            .contexts
                            .get(caller_ctx)
                            .and_then(|context| context.nodes.get(node))
                            .and_then(|node| match &node.kind {
                                NodeKind::Call { succ, .. } => succ
                                    .iter()
                                    .find(|(e, _)| *e == exit)
                                    .map(|(_, target)| *target),
                                NodeKind::Step { .. } => None,
                            });
                        if let Some(target) = target {
                            self.work.push((caller, target, screen, producer));
                        }
                    }
                }
                Target::Dead => {}
            }
        }
        Ok(())
    }

    /// The screen `step` shows when `screen` reaches it.
    fn apply(&mut self, step: StepRef, render: Option<&ConstValue>, screen: ScreenId) -> ScreenId {
        let Some(render) = render else {
            return screen;
        };
        if self.disabled.contains(&step) {
            return screen;
        }
        let key = (render.clone(), screen);
        if let Some(shown) = self.renders.get(&key) {
            return *shown;
        }
        let shown = self.evaluate(step, render, screen).unwrap_or(screen);
        self.renders.insert(key, shown);
        shown
    }

    /// Runs `render` on `screen`; reports its errors at the step.
    fn evaluate(
        &mut self,
        step: StepRef,
        render: &ConstValue,
        screen: ScreenId,
    ) -> Option<ScreenId> {
        let data = self.scenes.list.get(step.0)?;
        let mut input = Screen::empty(self.ctx.std);
        input.logical = self.screens.get(screen)?.clone();
        let site = Frame::new(data.b, self.ctx.env_for(data.item, Vec::new()));
        let mut machine = Machine::new(self.ctx);
        let callee = machine.thaw(render);
        match machine.call_value(&site, step.1, &callee, vec![Value::Screen(Box::new(input))]) {
            Ok(Value::Screen(output)) => Some(self.screens.intern(output.logical)),
            Ok(_) => None,
            Err(error) => {
                let ErrorKind::Fault(fault) = &error.kind else {
                    return None;
                };
                let mut fault = fault.clone();
                if fault.code == DiagnosticCode::EvalOverflow
                    && error
                        .span
                        .is_some_and(|span| span.file == self.ctx.std.time_file)
                {
                    fault = duration_overflow();
                }
                let site = self.render_site(step);
                let range = data.b.body.store.expr_range(site);
                let mut diagnostic = Diagnostic::new(
                    data.item.file.path(self.db).clone(),
                    range,
                    fault.code,
                    fault.message,
                );
                if let Some(span) = error.span
                    && (span.file != data.item.file || span.range != range)
                {
                    diagnostic = diagnostic.with_related(
                        span.file.path(self.db).clone(),
                        span.range,
                        "evaluation failed here",
                    );
                }
                if !self.diagnostics.contains(&diagnostic) {
                    self.diagnostics.push(diagnostic);
                }
                None
            }
        }
    }

    /// Where a step's render is written: the `x` of `render = x`, else the step.
    fn render_site(&self, step: StepRef) -> ExprId {
        let Some(data) = self.scenes.list.get(step.0) else {
            return step.1;
        };
        let store = &data.b.body.store;
        let Expr::Call { args, .. } = store.expr(step.1) else {
            return step.1;
        };
        let Some(Expr::Record { fields, .. }) = args.first().map(|arg| store.expr(*arg)) else {
            return step.1;
        };
        let Some((_, render)) = fields.iter().find(|(name, _)| name == "render") else {
            return step.1;
        };
        let option = &self.ctx.std.option;
        match store.expr(*render) {
            Expr::Call { callee, args } => {
                match (data.b.infer.paths.get(callee), args.as_slice()) {
                    (
                        Some(ValueRef::Fn(FnRef {
                            def: FnDefId::VariantCtor(adt, variant),
                            ..
                        })),
                        [inner],
                    ) if *adt == option.item && option.variant("Some") == Some(*variant) => *inner,
                    _ => *render,
                }
            }
            _ => *render,
        }
    }

    fn location(&self, step: StepRef) -> String {
        let Some(data) = self.scenes.list.get(step.0) else {
            return String::new();
        };
        let path = data.item.file.path(self.db);
        let (line, col) = line_col(self.db, path, data.b.body.store.expr_range(step.1).start());
        format!("{line}:{col}")
    }

    fn report_overflow(&mut self, culprit: StepRef, step: StepRef) {
        let Some(data) = self.scenes.list.get(culprit.0) else {
            return;
        };
        let site = self.render_site(culprit);
        let message = if culprit == step {
            format!(
                "this render keeps producing new screens for its own step: more than {SCREEN_CAP} \
                 different screens reach it, so the screens at this step have no bound"
            )
        } else {
            format!(
                "this render makes more than {SCREEN_CAP} different screens reach the step at {}: \
                 the screens at that step have no bound",
                self.location(step)
            )
        };
        let mut diagnostic = data.diagnostic(
            self.db,
            site,
            DiagnosticCode::AnalysisUnboundedScreens,
            message,
        );
        if culprit != step
            && let Some(target) = self.scenes.list.get(step.0)
        {
            diagnostic = diagnostic.with_related(
                target.item.file.path(self.db).clone(),
                target.b.body.store.expr_range(step.1),
                "the step whose screens grow",
            );
        }
        self.diagnostics.push(diagnostic);
    }

    /// Steps whose shown screen depends on the path, per group of instances entered from
    /// equal call stacks (up to screens).
    fn consistency(&mut self) {
        let mut seen: FxHashSet<(CtxId, Vec<ScreenId>)> = FxHashSet::default();
        let mut stack: Vec<(CtxId, Vec<ScreenId>)> = Vec::new();
        for root in &self.explored.roots {
            let group = (*root, vec![self.start]);
            if seen.insert(group.clone()) {
                stack.push(group);
            }
        }
        let mut reported: FxHashMap<StepRef, Vec<Producer>> = FxHashMap::default();
        let mut order: Vec<StepRef> = Vec::new();
        while let Some((ctx, group)) = stack.pop() {
            let insts: Vec<InstId> = group
                .iter()
                .filter_map(|screen| self.inst_index.get(&(ctx, *screen)).copied())
                .collect();
            let Some(context) = self.explored.contexts.get(ctx) else {
                continue;
            };
            for (node, data) in context.nodes.iter().enumerate() {
                match &data.kind {
                    NodeKind::Step { expr, render, .. } => {
                        let step = (context.scene, *expr);
                        if reported.contains_key(&step) {
                            continue;
                        }
                        let mut reaching: Vec<(ScreenId, Producer)> = Vec::new();
                        for inst in &insts {
                            if let Some(set) =
                                self.insts.get(*inst).and_then(|i| i.node_in.get(node))
                            {
                                reaching.extend(set.iter().map(|(s, p)| (*s, *p)));
                            }
                        }
                        reaching.sort_by_key(|(screen, _)| *screen);
                        let mut shown: Vec<(ScreenId, Producer)> = Vec::new();
                        for (screen, producer) in reaching {
                            let out = self.apply(step, render.as_ref(), screen);
                            if !shown.iter().any(|(s, _)| *s == out) {
                                shown.push((out, producer));
                            }
                        }
                        if shown.len() > 1 {
                            let mut producers: Vec<Producer> = Vec::new();
                            for (_, producer) in shown {
                                if !producers.contains(&producer) {
                                    producers.push(producer);
                                }
                            }
                            reported.insert(step, producers);
                            order.push(step);
                        }
                    }
                    NodeKind::Call { callee, .. } => {
                        let mut entered: Vec<ScreenId> = Vec::new();
                        for inst in &insts {
                            if let Some(set) =
                                self.insts.get(*inst).and_then(|i| i.node_in.get(node))
                            {
                                entered.extend(set.keys().copied());
                            }
                        }
                        entered.sort_unstable();
                        entered.dedup();
                        if !entered.is_empty() && seen.insert((*callee, entered.clone())) {
                            stack.push((*callee, entered));
                        }
                    }
                }
            }
        }
        // One report per merge: steps whose screens differ through the same renders are
        // reported at the first in source order.
        order.sort_by_key(|step| {
            let start = self
                .scenes
                .list
                .get(step.0)
                .map(|data| data.b.body.store.expr_range(step.1).start());
            (step.0, start)
        });
        let mut merges: FxHashSet<Vec<Producer>> = FxHashSet::default();
        for step in order {
            let Some(producers) = reported.get(&step) else {
                continue;
            };
            // The screens differ only through a render of a step already reported: the
            // difference comes from the same merge.
            if let [Some(producer)] = producers.as_slice()
                && *producer != step
                && reported.contains_key(producer)
            {
                continue;
            }
            let mut merge = producers.clone();
            merge.sort_by_key(|producer| {
                producer.map(|(scene, expr)| (scene, expr.into_raw().into_u32()))
            });
            if !merges.insert(merge) {
                continue;
            }
            let Some(data) = self.scenes.list.get(step.0) else {
                continue;
            };
            if !data.game {
                continue;
            }
            let mut diagnostic = data.diagnostic(
                self.db,
                step.1,
                DiagnosticCode::AnalysisInconsistentScreen,
                "the screen this step shows depends on the path that reached it: insert a \
                 keyframe after the paths merge, or choose the render by a story variable",
            );
            let note = if producers.len() > 1 {
                "on one of the paths, the screen was last changed here"
            } else {
                "the screens of the paths already differ when this render runs"
            };
            for producer in producers.iter().take(2).flatten() {
                let Some(source) = self.scenes.list.get(producer.0) else {
                    continue;
                };
                let site = self.render_site(*producer);
                diagnostic = diagnostic.with_related(
                    source.item.file.path(self.db).clone(),
                    source.b.body.store.expr_range(site),
                    note,
                );
            }
            self.diagnostics.push(diagnostic);
        }
    }
}
