//! The stage-2 termination check over the narrative IR.
//!
//! The check is path-insensitive: every branch arm counts as feasible. A cycle in a
//! scene's *stepless subgraph* (all nodes except `Say` and `Choice`) is a loop that can
//! repeat without showing a step; a cycle of scene calls made before any step is a
//! recursion that can do the same.

use crate::db::Db;
use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::input::Program;
use crate::ir::{IrScene, NarrativeIr, Node, NodeId, Op, SceneId, narrative_ir};
use crate::kis::game_hir;

#[salsa::tracked(returns(ref))]
pub fn termination(db: &dyn Db, program: Program) -> Vec<Diagnostic> {
    let ir = narrative_ir(db, program);
    let hir = game_hir(db, program);
    let scene_name = |scene: &IrScene| hir.scenes[scene.decl].name.clone();
    let analysis = analyze(ir);
    let mut diagnostics = Vec::new();

    for (scene_index, nodes) in analysis.loops {
        let Some(scene) = ir.scenes.get(scene_index) else {
            continue;
        };
        let Some(node) = nodes.first().and_then(|&n| node(scene, n)) else {
            continue;
        };
        diagnostics.push(node.span.diagnostic(
            db,
            DiagnosticCode::KisSteplessLoop,
            format!(
                "this loop in scene `{}` can repeat forever without reaching a step",
                scene_name(scene)
            ),
        ));
    }

    for cycle in analysis.call_cycles {
        let Some(scene) = ir.scenes.get(cycle.from) else {
            continue;
        };
        let Some(node) = node(scene, cycle.node) else {
            continue;
        };
        let names: Vec<String> = cycle
            .scenes
            .iter()
            .filter_map(|&s| ir.scenes.get(s))
            .map(|s| format!("`{}`", scene_name(s)))
            .collect();
        let message = if let [only] = names.as_slice() {
            format!("scene {only} can call itself forever without reaching a step")
        } else {
            format!(
                "scenes {} can call each other forever without reaching a step",
                names.join(", ")
            )
        };
        diagnostics.push(
            node.span
                .diagnostic(db, DiagnosticCode::KisSteplessCallCycle, message),
        );
    }
    diagnostics
}

/// The result of the termination analysis, independent of the database.
#[derive(Debug, PartialEq, Eq)]
struct Analysis {
    /// Per stepless loop: the scene index and the loop's nodes, the reporting node first.
    loops: Vec<(usize, Vec<NodeId>)>,
    call_cycles: Vec<CallCycle>,
}

/// A cycle of scene calls made before any step.
#[derive(Debug, PartialEq, Eq)]
struct CallCycle {
    /// Scenes in the cycle, in scene order.
    scenes: Vec<usize>,
    /// The reported call edge: the calling scene and its `Call` node.
    from: usize,
    node: NodeId,
}

fn node(scene: &IrScene, id: NodeId) -> Option<&Node> {
    usize::try_from(id.0).ok().and_then(|i| scene.nodes.get(i))
}

fn index(id: NodeId) -> usize {
    usize::try_from(id.0).unwrap_or(usize::MAX)
}

fn scene_index(id: SceneId) -> usize {
    usize::try_from(id.0).unwrap_or(usize::MAX)
}

fn is_stepless(node: &Node) -> bool {
    !matches!(node.op, Op::Say { .. } | Op::Choice { .. })
}

/// Successors of node `n` inside the stepless subgraph. `returns[s]` says whether scene
/// `s` may return without a step.
fn stepless_succ(scene: &IrScene, n: usize, returns: &[bool]) -> Vec<usize> {
    let Some(node) = scene.nodes.get(n) else {
        return Vec::new();
    };
    let targets: Vec<NodeId> = match &node.op {
        Op::Say { .. } | Op::Choice { .. } | Op::Exit => Vec::new(),
        Op::Jump { next, .. }
        | Op::Return { next, .. }
        | Op::Assign { next, .. }
        | Op::Add { next, .. }
        | Op::Sub { next, .. }
        | Op::MetaWrite { next } => vec![*next],
        Op::Branch { arms, .. } => arms.iter().map(|arm| arm.target).collect(),
        Op::Call {
            scene: callee,
            next,
            ..
        } => {
            if returns.get(scene_index(*callee)).copied().unwrap_or(false) {
                vec![*next]
            } else {
                Vec::new()
            }
        }
    };
    targets
        .into_iter()
        .map(index)
        .filter(|&t| scene.nodes.get(t).is_some_and(is_stepless))
        .collect()
}

/// Nodes of `scene`'s stepless subgraph reachable from its entry, as a membership mask.
fn reachable_from_entry(scene: &IrScene, returns: &[bool]) -> Vec<bool> {
    let mut seen = vec![false; scene.nodes.len()];
    let entry = index(scene.entry);
    if !scene.nodes.get(entry).is_some_and(is_stepless) {
        return seen;
    }
    let mut work = vec![entry];
    if let Some(s) = seen.get_mut(entry) {
        *s = true;
    }
    while let Some(n) = work.pop() {
        for t in stepless_succ(scene, n, returns) {
            if let Some(s) = seen.get_mut(t)
                && !*s
            {
                *s = true;
                work.push(t);
            }
        }
    }
    seen
}

/// Least fixpoint: scene `s` may return stepless iff its exit is reachable from its
/// entry in its stepless subgraph, given the current values for its callees.
fn may_return_stepless(scenes: &[IrScene]) -> Vec<bool> {
    let mut returns = vec![false; scenes.len()];
    loop {
        let mut changed = false;
        for (i, scene) in scenes.iter().enumerate() {
            if returns.get(i).copied().unwrap_or(true) {
                continue;
            }
            let reaches_exit = reachable_from_entry(scene, &returns)
                .get(index(scene.exit))
                .copied()
                .unwrap_or(false);
            if reaches_exit && let Some(r) = returns.get_mut(i) {
                *r = true;
                changed = true;
            }
        }
        if !changed {
            return returns;
        }
    }
}

fn analyze(ir: &NarrativeIr) -> Analysis {
    let returns = may_return_stepless(&ir.scenes);
    let mut loops = Vec::new();

    for (s, scene) in ir.scenes.iter().enumerate() {
        let include = |n: usize| scene.nodes.get(n).is_some_and(is_stepless);
        let succ = |n: usize| stepless_succ(scene, n, &returns);
        for component in cyclic_sccs(scene.nodes.len(), include, succ) {
            let key = |&n: &usize| {
                let start = scene.nodes.get(n).map(|node| node.span.range.start());
                (start, n)
            };
            let goto = component
                .iter()
                .copied()
                .filter(|&n| {
                    scene.nodes.get(n).is_some_and(|node| {
                        matches!(
                            node.op,
                            Op::Jump {
                                from_goto: true,
                                ..
                            }
                        )
                    })
                })
                .min_by_key(key);
            let Some(report) = goto.or_else(|| component.iter().copied().min_by_key(key)) else {
                continue;
            };
            let mut nodes = vec![report];
            nodes.extend(component.iter().copied().filter(|&n| n != report));
            let nodes = nodes
                .into_iter()
                .map(|n| NodeId(u32::try_from(n).unwrap_or(u32::MAX)))
                .collect();
            loops.push((s, nodes));
        }
    }

    // Scene call graph: A → B when A reaches a `Call{B}` before any step.
    let calls: Vec<Vec<(usize, usize)>> = ir
        .scenes
        .iter()
        .map(|scene| {
            let reach = reachable_from_entry(scene, &returns);
            let mut edges: Vec<(usize, usize)> = scene
                .nodes
                .iter()
                .enumerate()
                .filter(|&(n, _)| reach.get(n).copied().unwrap_or(false))
                .filter_map(|(n, node)| match node.op {
                    Op::Call { scene: callee, .. } => Some((scene_index(callee), n)),
                    _ => None,
                })
                .collect();
            edges.sort_by_key(|&(_, n)| {
                let start = scene.nodes.get(n).map(|node| node.span.range.start());
                (start, n)
            });
            edges
        })
        .collect();
    let call_succ = |s: usize| -> Vec<usize> {
        calls
            .get(s)
            .map(|edges| edges.iter().map(|&(callee, _)| callee).collect())
            .unwrap_or_default()
    };
    let mut call_cycles = Vec::new();
    for mut component in cyclic_sccs(ir.scenes.len(), |_| true, call_succ) {
        component.sort_unstable();
        // `calls` is sorted by node position, and `component` by scene order.
        let edge = component.iter().find_map(|&from| {
            calls.get(from).and_then(|edges| {
                edges
                    .iter()
                    .find(|(callee, _)| component.contains(callee))
                    .map(|&(_, n)| (from, n))
            })
        });
        let Some((from, n)) = edge else {
            continue;
        };
        call_cycles.push(CallCycle {
            scenes: component,
            from,
            node: NodeId(u32::try_from(n).unwrap_or(u32::MAX)),
        });
    }

    Analysis { loops, call_cycles }
}

/// Strongly connected components of the graph over `0..n` restricted to `include`d
/// vertices that contain a cycle (more than one vertex, or a self-edge). Iterative
/// Tarjan, so deep graphs cannot overflow the stack.
fn cyclic_sccs(
    n: usize,
    include: impl Fn(usize) -> bool,
    succ: impl Fn(usize) -> Vec<usize>,
) -> Vec<Vec<usize>> {
    const UNVISITED: usize = usize::MAX;
    let mut order = vec![UNVISITED; n];
    let mut low = vec![UNVISITED; n];
    let mut on_stack = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut counter: usize = 0;
    let mut result = Vec::new();
    let succ = |v: usize| -> Vec<usize> {
        succ(v)
            .into_iter()
            .filter(|&w| w < n && include(w))
            .collect()
    };

    for root in 0..n {
        if !include(root) || order.get(root) != Some(&UNVISITED) {
            continue;
        }
        // Frames: (vertex, its successors, next successor position).
        let mut frames: Vec<(usize, Vec<usize>, usize)> = Vec::new();
        let mut visit = |v: usize,
                         frames: &mut Vec<(usize, Vec<usize>, usize)>,
                         order: &mut Vec<usize>,
                         low: &mut Vec<usize>,
                         on_stack: &mut Vec<bool>,
                         stack: &mut Vec<usize>| {
            if let Some(o) = order.get_mut(v) {
                *o = counter;
            }
            if let Some(l) = low.get_mut(v) {
                *l = counter;
            }
            counter = counter.saturating_add(1);
            if let Some(s) = on_stack.get_mut(v) {
                *s = true;
            }
            stack.push(v);
            frames.push((v, succ(v), 0));
        };
        visit(
            root,
            &mut frames,
            &mut order,
            &mut low,
            &mut on_stack,
            &mut stack,
        );

        while let Some((v, succs, pos)) = frames.last_mut() {
            let v = *v;
            if let Some(&w) = succs.get(*pos) {
                *pos = pos.saturating_add(1);
                if order.get(w) == Some(&UNVISITED) {
                    visit(
                        w,
                        &mut frames,
                        &mut order,
                        &mut low,
                        &mut on_stack,
                        &mut stack,
                    );
                } else if on_stack.get(w).copied().unwrap_or(false) {
                    let w_order = order.get(w).copied().unwrap_or(UNVISITED);
                    if let Some(l) = low.get_mut(v) {
                        *l = (*l).min(w_order);
                    }
                }
                continue;
            }
            let self_edge = succs.contains(&v);
            frames.pop();
            let v_low = low.get(v).copied().unwrap_or(UNVISITED);
            if let Some((parent, _, _)) = frames.last()
                && let Some(l) = low.get_mut(*parent)
            {
                *l = (*l).min(v_low);
            }
            if order.get(v).copied() != Some(v_low) {
                continue;
            }
            let mut component = Vec::new();
            while let Some(w) = stack.pop() {
                if let Some(s) = on_stack.get_mut(w) {
                    *s = false;
                }
                component.push(w);
                if w == v {
                    break;
                }
            }
            if component.len() > 1 || self_edge {
                result.push(component);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use kagari_syntax::{TextRange, TextSize};
    use la_arena::{Idx, RawIdx};

    use super::{Analysis, CallCycle, analyze, cyclic_sccs};
    use crate::db::{OsFileSystem, RootDatabase};
    use crate::diagnostic::Span;
    use crate::input::{FilePath, SourceFile};
    use crate::ir::{IrBranchArm, IrScene, NarrativeIr, Node, NodeId, Op, SceneId};

    /// A scene from `(op, span start)` pairs; node 0 is the exit, `entry` is given.
    fn scene(file: SourceFile, entry: u32, ops: Vec<(Op, u32)>) -> IrScene {
        let nodes = std::iter::once((Op::Exit, 0))
            .chain(ops)
            .map(|(op, start)| Node {
                op,
                span: Span::new(file, TextRange::empty(TextSize::from(start))),
            })
            .collect();
        IrScene {
            decl: Idx::from_raw(RawIdx::from(0u32)),
            returns: None,
            nodes,
            entry: NodeId(entry),
            exit: NodeId(0),
        }
    }

    fn jump(next: u32, from_goto: bool) -> Op {
        Op::Jump {
            next: NodeId(next),
            from_goto,
        }
    }

    fn call(scene: u32, next: u32) -> Op {
        Op::Call {
            scene: SceneId(scene),
            assign: None,
            next: NodeId(next),
        }
    }

    #[test]
    fn analysis_follows_stepless_returns_transitively() {
        let db = RootDatabase::new(Arc::new(OsFileSystem));
        let file = SourceFile::new(&db, FilePath::Std("t.kis".into()), None);
        let ir = NarrativeIr {
            vars: Vec::new(),
            scenes: vec![
                // 0: label(1) → call 1 (2) → goto label (3); the loop is stepless only
                // because scene 1 may return through scene 2.
                scene(
                    file,
                    1,
                    vec![(jump(2, false), 10), (call(1, 3), 20), (jump(1, true), 30)],
                ),
                // 1: call 2 → exit.
                scene(file, 1, vec![(call(2, 0), 5)]),
                // 2: branch to a step or straight to the exit.
                scene(
                    file,
                    2,
                    vec![
                        (
                            Op::Say {
                                renders: Vec::new(),
                                next: NodeId(0),
                            },
                            3,
                        ),
                        (
                            Op::Branch {
                                vars: Vec::new(),
                                arms: vec![
                                    IrBranchArm {
                                        rows: Vec::new(),
                                        target: NodeId(1),
                                    },
                                    IrBranchArm {
                                        rows: Vec::new(),
                                        target: NodeId(0),
                                    },
                                ],
                            },
                            1,
                        ),
                    ],
                ),
                // 3 ↔ 4 call each other first; 4's later call to 3 sits after a step.
                scene(file, 1, vec![(call(4, 0), 7)]),
                scene(
                    file,
                    1,
                    vec![
                        (call(3, 2), 9),
                        (
                            Op::Say {
                                renders: Vec::new(),
                                next: NodeId(3),
                            },
                            10,
                        ),
                        (call(3, 0), 11),
                    ],
                ),
            ],
            entries: Vec::new(),
        };
        let mut analysis = analyze(&ir);
        // The reporting node comes first; the rest are in Tarjan order.
        for (_, nodes) in &mut analysis.loops {
            if let Some((_, rest)) = nodes.split_first_mut() {
                rest.sort_unstable();
            }
        }
        assert_eq!(
            analysis,
            Analysis {
                loops: vec![(0, vec![NodeId(3), NodeId(1), NodeId(2)])],
                call_cycles: vec![CallCycle {
                    scenes: vec![3, 4],
                    from: 3,
                    node: NodeId(1)
                }],
            }
        );
    }

    fn sorted(mut sccs: Vec<Vec<usize>>) -> Vec<Vec<usize>> {
        for c in &mut sccs {
            c.sort_unstable();
        }
        sccs.sort();
        sccs
    }

    #[test]
    fn sccs_report_only_cycles() {
        // 0 → 1 → 2 → 0, 2 → 3, 3 → 3, 4 → 5 (acyclic), 6 excluded (would close 5 → 6 → 4).
        let edges: Vec<Vec<usize>> = vec![
            vec![1],
            vec![2],
            vec![0, 3],
            vec![3],
            vec![5],
            vec![6],
            vec![4],
        ];
        let succ = |v: usize| edges.get(v).cloned().unwrap_or_default();
        let got = sorted(cyclic_sccs(7, |v| v != 6, succ));
        assert_eq!(got, vec![vec![0, 1, 2], vec![3]]);
    }

    #[test]
    fn sccs_handle_deep_chains() {
        let n = 200_000;
        let succ = |v: usize| {
            vec![if v.saturating_add(1) == n {
                0
            } else {
                v.saturating_add(1)
            }]
        };
        let got = cyclic_sccs(n, |_| true, succ);
        assert_eq!(got.len(), 1);
        assert_eq!(got.first().map(Vec::len), Some(n));
    }
}
