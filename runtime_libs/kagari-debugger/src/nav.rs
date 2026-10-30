//! Shortest paths through the story graph, from the node the tracker found to any node of a
//! target step.
//!
//! A path moves within a context along node targets, *descends* into a callee at a scene call
//! (when the target lies inside it), or passes a call through a **summary**: the cheapest way
//! from the callee's entry to one of its exits, after which the caller resumes at that exit's
//! successor. Exits of the contexts on the real call stack return to the real callers; exits of
//! a descended context lead nowhere (a path that must return uses the summary instead). The
//! cost of a path is the number of steps it passes.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use kagari_debug::{NodeKind, StoryGraph, Target};

use crate::tracker::At;

/// A choice or a draw a path depends on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Select `arm` at the choice node.
    Choice { at: At, arm: String },
    /// The draw node must draw `value` (an index into the graph's values), one of
    /// `outcomes`.
    Draw { at: At, value: u32, outcomes: u32 },
}

/// A shortest path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Path {
    pub actions: Vec<Action>,
    /// Steps passed, the target included.
    pub length: u32,
}

/// Per context, per exit: the fewest steps from the context's entry to that exit.
pub struct Summaries(Vec<Vec<Option<u32>>>);

impl Summaries {
    pub fn of(graph: &StoryGraph) -> Summaries {
        let mut sums = Summaries(
            graph
                .contexts
                .iter()
                .map(|c| vec![None; c.exits.len()])
                .collect(),
        );
        // Costs only fall; each round settles at least one more call level.
        for _ in 0..graph.contexts.len().saturating_add(2) {
            let mut changed = false;
            for context in 0..graph.contexts.len() {
                let Ok(context) = u32::try_from(context) else {
                    break;
                };
                let found = Local::search(graph, &sums, context).exit_costs();
                if let Some(slot) = sums.0.get_mut(context as usize) {
                    for (old, new) in slot.iter_mut().zip(found) {
                        if new.is_some_and(|new| old.is_none_or(|old| new < old)) {
                            *old = new;
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
        sums
    }

    fn cost(&self, context: u32, exit: u32) -> Option<u32> {
        *self.0.get(context as usize)?.get(exit as usize)?
    }
}

/// How an edge was taken.
#[derive(Clone, Debug)]
enum Label {
    /// A line went on, or a return.
    Plain,
    Choice(String),
    Draw {
        value: u32,
        outcomes: u32,
    },
    /// Into a callee's entry.
    Enter,
    /// Past a call, through the callee's cheapest way to `exit`.
    Summary {
        callee: u32,
        exit: u32,
    },
}

fn is_step(graph: &StoryGraph, context: u32, node: u32) -> bool {
    graph
        .node(context, node)
        .is_some_and(|n| n.kind.step().is_some())
}

/// The labelled successors of a node within its context: `(label, target, extra cost)`.
fn successors(
    graph: &StoryGraph,
    sums: &Summaries,
    context: u32,
    node: u32,
) -> Vec<(Label, Target, u32)> {
    let Some(data) = graph.node(context, node) else {
        return Vec::new();
    };
    match &data.kind {
        NodeKind::Say { next, .. } => vec![(Label::Plain, *next, 0)],
        NodeKind::Choice { arms, .. } => arms
            .iter()
            .map(|(arm, target)| (Label::Choice(arm.clone()), *target, 0))
            .collect(),
        NodeKind::Draw { draws } => {
            let outcomes = u32::try_from(draws.len()).unwrap_or(u32::MAX);
            draws
                .iter()
                .map(|(value, target)| {
                    (
                        Label::Draw {
                            value: *value,
                            outcomes,
                        },
                        *target,
                        0,
                    )
                })
                .collect()
        }
        NodeKind::Call { callee, exits } => exits
            .iter()
            .filter_map(|(exit, target)| {
                let cost = sums.cost(*callee, *exit)?;
                Some((
                    Label::Summary {
                        callee: *callee,
                        exit: *exit,
                    },
                    *target,
                    cost,
                ))
            })
            .collect(),
    }
}

/// A search within one context, from its entry, through summaries at calls.
struct Local {
    dist: HashMap<u32, u32>,
    parent: HashMap<u32, (Option<u32>, Label)>,
    /// Per exit: cost and the node (with its edge label) it is left from (`None`: the entry).
    exits: HashMap<u32, (u32, Option<u32>, Label)>,
    exit_count: usize,
}

impl Local {
    fn search(graph: &StoryGraph, sums: &Summaries, context: u32) -> Local {
        let mut local = Local {
            dist: HashMap::new(),
            parent: HashMap::new(),
            exits: HashMap::new(),
            exit_count: graph.context(context).map_or(0, |c| c.exits.len()),
        };
        let Some(ctx) = graph.context(context) else {
            return local;
        };
        let mut heap = BinaryHeap::new();
        local.reach(graph, context, None, Label::Plain, ctx.entry, 0, &mut heap);
        while let Some(Reverse((cost, node))) = heap.pop() {
            if local.dist.get(&node).is_some_and(|d| *d < cost) {
                continue;
            }
            for (label, target, extra) in successors(graph, sums, context, node) {
                local.reach(
                    graph,
                    context,
                    Some(node),
                    label,
                    target,
                    cost.saturating_add(extra),
                    &mut heap,
                );
            }
        }
        local
    }

    #[allow(clippy::too_many_arguments)]
    fn reach(
        &mut self,
        graph: &StoryGraph,
        context: u32,
        from: Option<u32>,
        label: Label,
        target: Target,
        cost: u32,
        heap: &mut BinaryHeap<Reverse<(u32, u32)>>,
    ) {
        match target {
            Target::Dead => {}
            Target::Exit(exit) => {
                if self.exits.get(&exit).is_none_or(|(c, ..)| cost < *c) {
                    self.exits.insert(exit, (cost, from, label));
                }
            }
            Target::Node(node) => {
                let cost = cost.saturating_add(u32::from(is_step(graph, context, node)));
                if self.dist.get(&node).is_none_or(|d| cost < *d) {
                    self.dist.insert(node, cost);
                    self.parent.insert(node, (from, label));
                    heap.push(Reverse((cost, node)));
                }
            }
        }
    }

    fn exit_costs(&self) -> Vec<Option<u32>> {
        (0..self.exit_count)
            .map(|exit| {
                let exit = u32::try_from(exit).ok()?;
                self.exits.get(&exit).map(|(cost, ..)| *cost)
            })
            .collect()
    }

    /// The actions from the entry to `exit`, summaries expanded.
    fn actions_to_exit(
        &self,
        graph: &StoryGraph,
        sums: &Summaries,
        context: u32,
        exit: u32,
        depth: u32,
    ) -> Vec<Action> {
        let Some((_, from, label)) = self.exits.get(&exit) else {
            return Vec::new();
        };
        let mut edges = vec![(*from, label.clone())];
        let mut at = *from;
        while let Some(node) = at {
            let Some((prev, label)) = self.parent.get(&node) else {
                break;
            };
            edges.push((*prev, label.clone()));
            at = *prev;
        }
        edges.reverse();
        let mut out = Vec::new();
        for (from, label) in edges {
            if let Some(from) = from {
                expand(
                    graph,
                    sums,
                    At {
                        context,
                        node: from,
                    },
                    &label,
                    depth,
                    &mut out,
                );
            }
        }
        out
    }
}

/// The actions of taking `label` from `at`.
fn expand(
    graph: &StoryGraph,
    sums: &Summaries,
    at: At,
    label: &Label,
    depth: u32,
    out: &mut Vec<Action>,
) {
    match label {
        Label::Plain | Label::Enter => {}
        Label::Choice(arm) => out.push(Action::Choice {
            at,
            arm: arm.clone(),
        }),
        Label::Draw { value, outcomes } => out.push(Action::Draw {
            at,
            value: *value,
            outcomes: *outcomes,
        }),
        Label::Summary { callee, exit } => {
            // A summary's path is cheaper than its caller's, so the recursion ends; the depth
            // bound guards a malformed graph.
            if depth < 4096 {
                let inner = Local::search(graph, sums, *callee);
                out.extend(inner.actions_to_exit(
                    graph,
                    sums,
                    *callee,
                    *exit,
                    depth.saturating_add(1),
                ));
            }
        }
    }
}

/// Where a state's exits go: to a real caller (by its level in the real stack), or nowhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Frame {
    Real(u32),
    Nested,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct State {
    frame: Frame,
    context: u32,
    node: u32,
}

/// The shortest path from the top of `real` (the tracker's stack) to a node of the step
/// `target` (`{path}#{n}`); `None` when the graph has none.
pub fn shortest(graph: &StoryGraph, sums: &Summaries, real: &[At], target: &str) -> Option<Path> {
    let top = real.last()?;
    let level = u32::try_from(real.len().checked_sub(1)?).ok()?;
    let start = State {
        frame: Frame::Real(level),
        context: top.context,
        node: top.node,
    };
    let mut dist: HashMap<State, u32> = HashMap::new();
    let mut parent: HashMap<State, (State, Label)> = HashMap::new();
    let mut heap = BinaryHeap::new();
    dist.insert(start, 0);
    heap.push(Reverse((0u32, start)));
    while let Some(Reverse((cost, state))) = heap.pop() {
        if dist.get(&state).is_some_and(|d| *d < cost) {
            continue;
        }
        if graph.step_id(state.context, state.node).as_deref() == Some(target) {
            return Some(Path {
                actions: path_actions(graph, sums, &parent, state),
                length: cost,
            });
        }
        let mut edges: Vec<(Label, Frame, u32, Target, u32)> =
            successors(graph, sums, state.context, state.node)
                .into_iter()
                .map(|(label, target, extra)| (label, state.frame, state.context, target, extra))
                .collect();
        // Descending: the target may lie in the callee; its exits then lead nowhere.
        if let Some(NodeKind::Call { callee, .. }) =
            graph.node(state.context, state.node).map(|n| &n.kind)
            && let Some(entry) = graph.context(*callee).map(|c| c.entry)
        {
            edges.push((Label::Enter, Frame::Nested, *callee, entry, 0));
        }
        for (label, frame, context, target, extra) in edges {
            let Some((next, steps)) = resolve(graph, real, frame, context, target) else {
                continue;
            };
            let cost = cost.saturating_add(extra).saturating_add(steps);
            if dist.get(&next).is_none_or(|d| cost < *d) {
                dist.insert(next, cost);
                parent.insert(next, (state, label));
                heap.push(Reverse((cost, next)));
            }
        }
    }
    None
}

/// The state a target leads to, following exits up the real stack; with the steps it adds.
fn resolve(
    graph: &StoryGraph,
    real: &[At],
    mut frame: Frame,
    mut context: u32,
    mut target: Target,
) -> Option<(State, u32)> {
    for _ in 0..=real.len() {
        match target {
            Target::Dead => return None,
            Target::Node(node) => {
                return Some((
                    State {
                        frame,
                        context,
                        node,
                    },
                    u32::from(is_step(graph, context, node)),
                ));
            }
            Target::Exit(exit) => {
                let Frame::Real(level) = frame else {
                    return None;
                };
                let caller_level = level.checked_sub(1)?;
                let caller = real.get(caller_level as usize)?;
                let NodeKind::Call { exits, .. } = &graph.node(caller.context, caller.node)?.kind
                else {
                    return None;
                };
                target = exits.iter().find(|(e, _)| *e == exit)?.1;
                frame = Frame::Real(caller_level);
                context = caller.context;
            }
        }
    }
    None
}

fn path_actions(
    graph: &StoryGraph,
    sums: &Summaries,
    parent: &HashMap<State, (State, Label)>,
    end: State,
) -> Vec<Action> {
    let mut edges = Vec::new();
    let mut at = end;
    while let Some((prev, label)) = parent.get(&at) {
        edges.push((*prev, label.clone()));
        at = *prev;
    }
    edges.reverse();
    let mut out = Vec::new();
    for (from, label) in edges {
        let at = At {
            context: from.context,
            node: from.node,
        };
        expand(graph, sums, at, &label, 0, &mut out);
    }
    out
}
