//! The keyframe-placement check (opt-in, `CompileOptions::keyframe_check`): loading a save or
//! jumping to a step replays renders from the last keyframe before it, so the steps the
//! player can land on directly should have keyframe renders. Those are every choice and the
//! first steps of each entry scene.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::db::Db;
use crate::diagnostic::{Diag, DiagnosticCode};
use crate::eval::{ConstValue, RenderId, RenderInfo, const_eval};
use crate::hir::ExprId;
use crate::input::Program;
use crate::ty::{FnDefId, FnRef};

use super::Scenes;
use super::explore::{CtxId, ExitId, Explored, NodeKind, Target};

/// A step: its scene (index into `Scenes::list`) and its `push_step` call.
type StepRef = (usize, ExprId);

pub(super) fn check(
    db: &dyn Db,
    program: Program,
    explored: &Explored,
    scenes: &Scenes<'_>,
) -> Vec<Diag> {
    let renders = &const_eval(db, program).renders;
    let mut reported: FxHashSet<StepRef> = FxHashSet::default();
    let mut out = Vec::new();
    let mut report = |step: StepRef, message: String, out: &mut Vec<Diag>| {
        let Some(data) = scenes.list.get(step.0) else {
            return;
        };
        if data.game && reported.insert(step) {
            out.push(data.diagnostic(step.1, DiagnosticCode::AnalysisMissingKeyframe, message));
        }
    };
    for context in &explored.contexts {
        for node in &context.nodes {
            if let NodeKind::Step {
                expr,
                render,
                choice: true,
                ..
            } = &node.kind
                && !is_keyframe(renders, render.as_ref())
            {
                report(
                    (context.scene, *expr),
                    "this choice has no keyframe render: loading or jumping here replays renders \
                     from an earlier keyframe"
                        .to_owned(),
                    &mut out,
                );
            }
        }
    }
    for &root in &explored.roots {
        let Some(context) = explored.context(root) else {
            continue;
        };
        let Some(name) = scenes.list.get(context.scene).map(|data| data.name(db)) else {
            continue;
        };
        let mut first = First {
            explored,
            steps: Vec::new(),
            memo: FxHashMap::default(),
        };
        first.from(root, context.entry);
        for (step, render) in first.steps {
            if !is_keyframe(renders, render.as_ref()) {
                report(
                    step,
                    format!("the first step of entry scene `{name}` has no keyframe render"),
                    &mut out,
                );
            }
        }
    }
    out
}

/// Whether `render` is a render whose output does not depend on its input screen. No
/// render is not a keyframe.
fn is_keyframe(renders: &FxHashMap<RenderId, RenderInfo>, render: Option<&ConstValue>) -> bool {
    let id = match render {
        Some(ConstValue::Fn(FnRef {
            def: FnDefId::Item(item),
            ..
        })) => RenderId::Fn(*item),
        Some(ConstValue::Closure { owner, expr }) => RenderId::Closure(*owner, *expr),
        _ => return false,
    };
    renders.get(&id).is_some_and(|info| info.keyframe)
}

/// The steps reached from a target without passing another step: through random draws,
/// into called scenes and back out of them.
struct First<'a> {
    explored: &'a Explored,
    steps: Vec<(StepRef, Option<ConstValue>)>,
    /// Per visited `(context, target)`, the exits of that context reached from it.
    memo: FxHashMap<(CtxId, Target), Vec<ExitId>>,
}

impl First<'_> {
    /// Records the first steps from `target` in `ctx`; returns the exits of `ctx` reached
    /// without a step.
    fn from(&mut self, ctx: CtxId, target: Target) -> Vec<ExitId> {
        if let Some(exits) = self.memo.get(&(ctx, target)) {
            return exits.clone();
        }
        // A cycle without a step is a termination error; break it here.
        self.memo.insert((ctx, target), Vec::new());
        let mut exits = Vec::new();
        let explored = self.explored;
        match target {
            Target::Exit(exit) => exits.push(exit),
            Target::Dead => {}
            Target::Node(node) => {
                let Some(context) = explored.context(ctx) else {
                    return exits;
                };
                match context.node(node).map(|node| &node.kind) {
                    Some(NodeKind::Step { expr, render, .. }) => {
                        let step = ((context.scene, *expr), render.clone());
                        if !self.steps.contains(&step) {
                            self.steps.push(step);
                        }
                    }
                    Some(NodeKind::Rng { succ, .. }) => {
                        for next in succ {
                            for exit in self.from(ctx, *next) {
                                if !exits.contains(&exit) {
                                    exits.push(exit);
                                }
                            }
                        }
                    }
                    Some(NodeKind::Call { callee, succ }) => {
                        let entry = explored
                            .context(*callee)
                            .map_or(Target::Dead, |callee| callee.entry);
                        for callee_exit in self.from(*callee, entry) {
                            for (exit, next) in succ {
                                if *exit != callee_exit {
                                    continue;
                                }
                                for exit in self.from(ctx, *next) {
                                    if !exits.contains(&exit) {
                                        exits.push(exit);
                                    }
                                }
                            }
                        }
                    }
                    None => {}
                }
            }
        }
        self.memo.insert((ctx, target), exits.clone());
        exits
    }
}
