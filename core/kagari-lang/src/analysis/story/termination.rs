//! Termination (`docs/syntax/src/narrative/scenes.md#termination`): every `loop` and
//! `while` of a scene, and every cycle of scene calls, passes through a step.
//!
//! A scene *may return without a step* when a path from its entry to a return passes no
//! step and calls only such scenes; the least fixpoint over all scenes is computed first.
//! A call of such a scene counts as stepless, like every instant code. Every branch counts
//! as possible.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::db::Db;
use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::hir::ExprId;
use crate::signatures::ProgramSignatures;
use crate::ty::{FnDefId, Ty, TyNode};

use super::Scenes;
use super::cfg::{BlockId, Callee, StoryBody, Term};

/// The scenes a call can reach: its callee, or every scene a `scene fn` pointer of its
/// type can hold. A parameter or return type that mentions a generic parameter (on either
/// side) matches any type in that position; the other positions must be equal.
pub(super) fn callees(
    scenes: &Scenes<'_>,
    sigs: &ProgramSignatures,
    callee: &Callee,
) -> Vec<usize> {
    let has_params = |ty: &Ty| {
        let mut found = false;
        ty.walk(&mut |node| found |= matches!(node, TyNode::Ty(Ty::Param { .. })));
        found
    };
    let compatible = |a: &Ty, b: &Ty| a == b || has_params(a) || has_params(b);
    match callee {
        Callee::Direct(fn_ref) => match fn_ref.def {
            FnDefId::Item(item) => scenes.index.get(&item).copied().into_iter().collect(),
            _ => Vec::new(),
        },
        Callee::Value {
            ty: Ty::FnPtr { params, ret, .. },
            ..
        } => scenes
            .list
            .iter()
            .enumerate()
            .filter(|(_, scene)| {
                sigs.fn_sig(scene.item).is_some_and(|sig| {
                    sig.params.len() == params.len()
                        && sig.params.iter().zip(params).all(|(a, b)| compatible(a, b))
                        && compatible(&sig.ret, ret)
                })
            })
            .map(|(index, _)| index)
            .collect(),
        // Not a pointer type: stage 3 rejected the call already.
        Callee::Value { .. } => Vec::new(),
    }
}

/// What a scene reaches from `start` without passing a step.
struct Reach {
    blocks: FxHashSet<BlockId>,
    returns: bool,
    /// Scene calls reached: the callee and the call expression.
    calls: Vec<(usize, ExprId)>,
    /// Whether `start` itself was reached again.
    cycles: bool,
}

/// `within`: the blocks of a loop body, outside which the search stops (leaving the loop
/// and entering it again on a lap of an enclosing loop is not a path around it).
fn reach(
    body: &StoryBody,
    calls: &FxHashMap<BlockId, Vec<usize>>,
    may_return: &[bool],
    start: BlockId,
    within: Option<&FxHashSet<BlockId>>,
) -> Reach {
    let mut out = Reach {
        blocks: FxHashSet::default(),
        returns: false,
        calls: Vec::new(),
        cycles: false,
    };
    let mut stack = vec![start];
    out.blocks.insert(start);
    while let Some(block) = stack.pop() {
        let Some(data) = body.blocks.get(block) else {
            continue;
        };
        let mut next = match &data.term {
            Term::Step { .. } => Vec::new(),
            Term::Call { expr, next, .. } => {
                let targets = calls.get(&block).map(Vec::as_slice).unwrap_or_default();
                out.calls
                    .extend(targets.iter().map(|target| (*target, *expr)));
                if targets
                    .iter()
                    .any(|target| may_return.get(*target).copied().unwrap_or(false))
                {
                    vec![*next]
                } else {
                    Vec::new()
                }
            }
            Term::Return { .. } => {
                out.returns = true;
                Vec::new()
            }
            term => StoryBody::successors(term),
        };
        // An atomic `break`, `continue` or `return` leaves before the terminator runs.
        next.extend(data.exits.iter().flatten().copied());
        for next in next {
            if within.is_some_and(|region| !region.contains(&next)) {
                continue;
            }
            if next == start {
                out.cycles = true;
            }
            if out.blocks.insert(next) {
                stack.push(next);
            }
        }
    }
    out
}

/// Reports stepless loops and stepless call cycles; returns whether there were none.
pub(super) fn check(
    db: &dyn Db,
    scenes: &Scenes<'_>,
    sigs: &ProgramSignatures,
    out: &mut Vec<Diagnostic>,
) -> bool {
    let calls: Vec<FxHashMap<BlockId, Vec<usize>>> = scenes
        .list
        .iter()
        .map(|scene| {
            scene
                .body
                .blocks
                .iter()
                .enumerate()
                .filter_map(|(block, data)| match &data.term {
                    Term::Call { callee, .. } => Some((block, callees(scenes, sigs, callee))),
                    _ => None,
                })
                .collect()
        })
        .collect();
    let mut may_return = vec![false; scenes.list.len()];
    loop {
        let mut changed = false;
        for (index, scene) in scenes.list.iter().enumerate() {
            if may_return.get(index).copied().unwrap_or(true) {
                continue;
            }
            let Some(scene_calls) = calls.get(index) else {
                continue;
            };
            if reach(
                &scene.body,
                scene_calls,
                &may_return,
                scene.body.entry,
                None,
            )
            .returns
                && let Some(flag) = may_return.get_mut(index)
            {
                *flag = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let before = out.len();
    let mut edges: Vec<Vec<(usize, ExprId)>> = Vec::with_capacity(scenes.list.len());
    for (index, scene) in scenes.list.iter().enumerate() {
        let Some(scene_calls) = calls.get(index) else {
            edges.push(Vec::new());
            continue;
        };
        for (expr, header, region) in &scene.body.loops {
            if reach(&scene.body, scene_calls, &may_return, *header, Some(region)).cycles
                && scene.game
            {
                out.push(scene.diagnostic(
                    db,
                    *expr,
                    DiagnosticCode::AnalysisSteplessLoop,
                    "this loop can repeat without passing a step: every path around a loop \
                     in a scene must run `s!`, `choice!` or a scene that runs one",
                ));
            }
        }
        edges.push(
            reach(
                &scene.body,
                scene_calls,
                &may_return,
                scene.body.entry,
                None,
            )
            .calls,
        );
    }

    for component in strongly_connected(&edges) {
        let members: FxHashSet<usize> = component.iter().copied().collect();
        let mut sites: Vec<(usize, ExprId)> = Vec::new();
        for &from in &component {
            for &(to, expr) in edges.get(from).map(Vec::as_slice).unwrap_or_default() {
                if members.contains(&to) && !sites.contains(&(from, expr)) {
                    sites.push((from, expr));
                }
            }
        }
        let cyclic = component.len() > 1 || !sites.is_empty();
        if !cyclic {
            continue;
        }
        sites.sort_by_key(|(from, expr)| {
            let scene = scenes.list.get(*from);
            (
                scene.map(|scene| scene.item.index),
                scene.map(|scene| scene.b.body.store.expr_range(*expr).start()),
            )
        });
        let Some(&(first_scene, first_expr)) = sites.first() else {
            continue;
        };
        let Some(scene) = scenes.list.get(first_scene) else {
            continue;
        };
        if !scene.game {
            continue;
        }
        let mut names: Vec<String> = component
            .iter()
            .filter_map(|index| scenes.list.get(*index))
            .map(|scene| format!("`{}`", scene.name(db)))
            .collect();
        names.sort();
        let message = if names.len() == 1 {
            format!(
                "the scene {} can call itself before passing a step: the call stack would grow \
                 forever without input",
                names.join("")
            )
        } else {
            format!(
                "the scenes {} can call each other before passing a step: the call stack would \
                 grow forever without input",
                join_names(&names)
            )
        };
        let mut diagnostic = scene.diagnostic(
            db,
            first_expr,
            DiagnosticCode::AnalysisSteplessRecursion,
            message,
        );
        for &(from, expr) in sites.iter().skip(1) {
            if let Some(other) = scenes.list.get(from) {
                diagnostic = diagnostic.with_related(
                    other.b.item.file.path(db).clone(),
                    other.b.body.store.expr_range(expr),
                    "a call on the cycle",
                );
            }
        }
        out.push(diagnostic);
    }
    out.len() == before
}

/// `a`, `a and b`, `a, b and c`.
pub(super) fn join_names(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// Tarjan's strongly connected components, each in discovery order.
fn strongly_connected(edges: &[Vec<(usize, ExprId)>]) -> Vec<Vec<usize>> {
    struct State {
        index: Vec<Option<usize>>,
        low: Vec<usize>,
        on_stack: Vec<bool>,
        stack: Vec<usize>,
        next: usize,
        out: Vec<Vec<usize>>,
    }
    let count = edges.len();
    let mut state = State {
        index: vec![None; count],
        low: vec![0; count],
        on_stack: vec![false; count],
        stack: Vec::new(),
        next: 0,
        out: Vec::new(),
    };
    // Iterative DFS: (node, next edge to visit).
    for root in 0..count {
        if state.index.get(root).copied().flatten().is_some() {
            continue;
        }
        let mut work: Vec<(usize, usize)> = vec![(root, 0)];
        while let Some((node, edge)) = work.pop() {
            if edge == 0 {
                if let Some(slot) = state.index.get_mut(node) {
                    *slot = Some(state.next);
                }
                if let Some(slot) = state.low.get_mut(node) {
                    *slot = state.next;
                }
                state.next = state.next.saturating_add(1);
                state.stack.push(node);
                if let Some(slot) = state.on_stack.get_mut(node) {
                    *slot = true;
                }
            }
            let succs = edges.get(node).map(Vec::as_slice).unwrap_or_default();
            if let Some(&(succ, _)) = succs.get(edge) {
                work.push((node, edge.saturating_add(1)));
                match state.index.get(succ).copied().flatten() {
                    None => work.push((succ, 0)),
                    Some(succ_index) => {
                        if state.on_stack.get(succ).copied().unwrap_or(false) {
                            let low = state.low.get(node).copied().unwrap_or(0).min(succ_index);
                            if let Some(slot) = state.low.get_mut(node) {
                                *slot = low;
                            }
                        }
                    }
                }
                continue;
            }
            // All edges done: propagate to the parent and pop a component.
            if let Some(&(parent, _)) = work.last() {
                let low = state
                    .low
                    .get(parent)
                    .copied()
                    .unwrap_or(0)
                    .min(state.low.get(node).copied().unwrap_or(0));
                if let Some(slot) = state.low.get_mut(parent) {
                    *slot = low;
                }
            }
            if state.low.get(node).copied() == state.index.get(node).copied().flatten() {
                let mut component = Vec::new();
                while let Some(member) = state.stack.pop() {
                    if let Some(slot) = state.on_stack.get_mut(member) {
                        *slot = false;
                    }
                    component.push(member);
                    if member == node {
                        break;
                    }
                }
                component.reverse();
                state.out.push(component);
            }
        }
    }
    state.out
}
