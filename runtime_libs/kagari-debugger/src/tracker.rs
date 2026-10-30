//! Following the story through the explored story graph: the node of the graph that matches
//! the story's current state, with the real call stack above it.
//!
//! The walk is exact: from a step node, the input (a line goes on, a choice's arm) picks the
//! next target; a scene call enters its callee's context; a draw follows the value the fiber
//! actually drew (`StoryFiber::draws`); an exit returns to the caller's call node, whose
//! successor for that exit is known. The step the story reaches must be the node the walk
//! reaches, or the graph does not describe this program (a stale artifact) and the tracker is
//! lost.

use kagari_debug::{NodeKind, StoryGraph, Target};
use kagari_engine::debug::{Arrival, GameView};

use crate::values::{Names, to_graph};

/// A graph node: a context and a node in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct At {
    pub context: u32,
    pub node: u32,
}

/// Where the story is in the graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Track {
    /// No step observed yet.
    Unstarted,
    /// The real stack: the call nodes of the callers, bottom first, then the step node.
    At(Vec<At>),
    /// The story's state is not in the graph.
    Lost(String),
}

/// Follows a drive's steps through the graph.
pub struct Tracker<'g> {
    graph: &'g StoryGraph,
    pub track: Track,
}

impl<'g> Tracker<'g> {
    pub fn new(graph: &'g StoryGraph) -> Tracker<'g> {
        Tracker {
            graph,
            track: Track::Unstarted,
        }
    }

    /// The story entered the step of `view`.
    pub fn observe(&mut self, view: &GameView<'_>) {
        if matches!(self.track, Track::Lost(_)) {
            return;
        }
        let names = Names {
            program: view.program,
            types: view.types,
        };
        let draws: Vec<kagari_debug::Value> = match view.fiber {
            Some(fiber) => match fiber
                .draws()
                .iter()
                .map(|v| to_graph(names, v))
                .collect::<Option<_>>()
            {
                Some(draws) => draws,
                None => return self.lose("a drawn value has no graph form"),
            },
            None => Vec::new(),
        };
        let walked = match (view.arrival, &self.track) {
            (Arrival::Current, _) => return,
            (Arrival::Start, _) => self.start(view, names),
            (Arrival::Next, Track::At(stack)) => self.resume(stack.clone(), None),
            (Arrival::Select(arm), Track::At(stack)) => self.resume(stack.clone(), Some(arm)),
            (_, _) => Err("the story's start was not observed".to_owned()),
        }
        .and_then(|(stack, target)| self.walk(stack, target, &draws));
        let stack = match walked {
            Ok(stack) => stack,
            Err(reason) => return self.lose(&reason),
        };
        // The walk must reach the step the story reached.
        let Some(step) = view.step.as_ref() else {
            return self.lose("the story returned where the graph has a step");
        };
        let expected = format!(
            "{}#{}",
            view.program.function_path(step.step.func),
            step.step.id
        );
        let reached = stack
            .last()
            .and_then(|at| self.graph.step_id(at.context, at.node));
        if reached.as_deref() != Some(expected.as_str()) {
            return self.lose(&format!(
                "the story reached `{expected}` where the graph has `{}`",
                reached.as_deref().unwrap_or("no step")
            ));
        }
        self.track = Track::At(stack);
    }

    fn lose(&mut self, reason: &str) {
        self.track = Track::Lost(reason.to_owned());
    }

    /// The root context of the entry scene with the trace's arguments.
    fn start(&self, view: &GameView<'_>, names: Names<'_>) -> Result<(Vec<At>, Target), String> {
        let args: Option<Vec<kagari_debug::Value>> =
            view.trace.args.iter().map(|v| to_graph(names, v)).collect();
        let args = args.ok_or("an entry argument has no graph form")?;
        let root = self
            .graph
            .roots
            .iter()
            .copied()
            .find(|&root| {
                self.graph.context(root).is_some_and(|ctx| {
                    ctx.path == view.trace.entry
                        && ctx.args.len() == args.len()
                        && ctx
                            .args
                            .iter()
                            .zip(&args)
                            .all(|(id, arg)| self.graph.value(*id) == Some(arg))
                })
            })
            .ok_or_else(|| {
                format!(
                    "the graph has no entry `{}` with these arguments",
                    view.trace.entry
                )
            })?;
        let entry = self.graph.context(root).map(|ctx| ctx.entry);
        Ok((
            vec![At {
                context: root,
                node: u32::MAX,
            }],
            entry.ok_or("a malformed graph")?,
        ))
    }

    /// The target the step at the top of `stack` resumes to.
    fn resume(&self, stack: Vec<At>, arm: Option<&String>) -> Result<(Vec<At>, Target), String> {
        let top = *stack.last().ok_or("an empty stack")?;
        let node = self
            .graph
            .node(top.context, top.node)
            .ok_or("a malformed graph")?;
        let target = match (&node.kind, arm) {
            (NodeKind::Say { next, .. }, None) => *next,
            (NodeKind::Choice { arms, .. }, Some(arm)) => arms
                .iter()
                .find(|(id, _)| id == arm)
                .map(|(_, target)| *target)
                .ok_or_else(|| format!("the graph's choice has no arm `{arm}`"))?,
            _ => return Err("the graph's step does not match the story's input".to_owned()),
        };
        Ok((stack, target))
    }

    /// Walks from `target` in the context at the top of `stack` to the next step node.
    fn walk(
        &self,
        mut stack: Vec<At>,
        mut target: Target,
        draws: &[kagari_debug::Value],
    ) -> Result<Vec<At>, String> {
        let mut draws = draws.iter();
        // A walk passes each node at most once between two draws.
        let limit = self
            .graph
            .contexts
            .iter()
            .map(|c| c.nodes.len().saturating_add(c.exits.len()))
            .sum::<usize>()
            .saturating_add(1)
            .saturating_mul(draws.len().saturating_add(1));
        for _ in 0..limit {
            let top = stack.last_mut().ok_or("an empty stack")?;
            let context = top.context;
            match target {
                Target::Dead => {
                    return Err(
                        "the graph has no state past this point (a fault, or a path \
                                the analysis gave up)"
                            .to_owned(),
                    );
                }
                Target::Node(n) => {
                    top.node = n;
                    let node = self.graph.node(context, n).ok_or("a malformed graph")?;
                    match &node.kind {
                        NodeKind::Say { .. } | NodeKind::Choice { .. } => return Ok(stack),
                        NodeKind::Call { callee, .. } => {
                            let entry = self
                                .graph
                                .context(*callee)
                                .ok_or("a malformed graph")?
                                .entry;
                            stack.push(At {
                                context: *callee,
                                node: u32::MAX,
                            });
                            target = entry;
                        }
                        NodeKind::Draw { draws: outcomes } => {
                            let drawn = draws
                                .next()
                                .ok_or("the graph draws where the story drew nothing")?;
                            target = outcomes
                                .iter()
                                .find(|(value, _)| self.graph.value(*value) == Some(drawn))
                                .map(|(_, target)| *target)
                                .ok_or("the story drew a value the graph does not have")?;
                        }
                    }
                }
                Target::Exit(e) => {
                    stack.pop();
                    let caller = *stack
                        .last()
                        .ok_or("the graph's entry scene returned where the story has a step")?;
                    let node = self
                        .graph
                        .node(caller.context, caller.node)
                        .ok_or("a malformed graph")?;
                    let NodeKind::Call { exits, .. } = &node.kind else {
                        return Err("a malformed graph".to_owned());
                    };
                    target = exits
                        .iter()
                        .find(|(exit, _)| *exit == e)
                        .map(|(_, target)| *target)
                        .ok_or("a malformed graph")?;
                }
            }
        }
        Err("the graph loops without a step".to_owned())
    }
}
