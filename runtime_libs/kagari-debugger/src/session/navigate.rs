//! `nav`: the story's node in the story graph, the shortest path from it, and the seed search
//! for the path's draws.

use std::rc::Rc;

use kagari_debug::StoryGraph;
use kagari_engine::debug::{StepId, Trace};

use super::pilots::{ObservePilot, RealizePilot};
use super::{Debugger, Result, Target, parse_step};
use crate::nav::{self, Action, Path, Summaries};
use crate::protocol::{NavBody, PathItem, SeedSearch};
use crate::tracker::{Track, Tracker};
use crate::values::graph_display;

/// The seeds `nav` tries by default.
pub const DEFAULT_SEEDS: u32 = 1000;

/// What a tracked position is cached by: the trace without its arguments, which do not change
/// within a session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TraceKey {
    entry: String,
    seed: u64,
    choices: Vec<String>,
    steps: u32,
}

impl TraceKey {
    fn of(trace: &Trace) -> TraceKey {
        TraceKey {
            entry: trace.entry.clone(),
            seed: trace.seed,
            choices: trace.choices.clone(),
            steps: trace.steps_after_last_choice,
        }
    }
}

impl Debugger {
    /// Whether `step` can be reached from the current state, and one shortest way.
    pub fn nav(&mut self, t: &mut Target<'_>, step: &str, seeds: Option<u32>) -> Result<NavBody> {
        let target = parse_step(t.engine.program(), step)?;
        let target_name = t.engine.program().step_path(target);
        let artifact = self
            .artifact
            .clone()
            .ok_or("`nav` needs the debug artifact: build with `kagari build --profile debug`")?;
        let graph = artifact
            .graph
            .as_ref()
            .ok_or("the debug artifact has no story graph")?;
        let trace = Self::trace(t)?;
        let real = match self.track(t, &trace)? {
            Track::At(stack) => stack,
            Track::Lost(reason) => {
                return Ok(unknown(format!(
                    "the current state is not in the story graph ({reason}); the artifact may \
                     be stale, or the state lies past a cap"
                )));
            }
            Track::Unstarted => {
                return Ok(unknown(
                    "the current state is not in the story graph".to_owned(),
                ));
            }
        };
        let sums = self
            .summaries
            .get_or_insert_with(|| Rc::new(Summaries::of(graph)))
            .clone();
        let Some(path) = nav::shortest(graph, &sums, &real, &target_name) else {
            return Ok(if graph.complete {
                NavBody {
                    answer: "unreachable".to_owned(),
                    reason: Some("the story graph is complete".to_owned()),
                    ..NavBody::default()
                }
            } else {
                unknown(incomplete_reason(graph))
            });
        };
        let mut body = reachable(graph, &path);
        if let Some(mut search) = body.seed.take() {
            search.limit = seeds.unwrap_or(DEFAULT_SEEDS);
            self.search_seed(t, &trace, &body.push, target, path.length, &mut search);
            body.seed = Some(search);
        }
        Ok(body)
    }

    /// Replays the current trace plus the path's arms with other seeds (the current one first,
    /// then 0, 1, …) until one reaches `target`, at most `search.limit` tries.
    fn search_seed(
        &self,
        t: &mut Target<'_>,
        trace: &Trace,
        arms: &[String],
        target: StepId,
        length: u32,
        search: &mut SeedSearch,
    ) {
        search.current = trace.seed;
        let seeds = std::iter::once(trace.seed)
            .chain((0..u64::MAX).filter(|s| *s != trace.seed))
            .take(search.limit as usize);
        for seed in seeds {
            search.tried = search.tried.saturating_add(1);
            let attempt = Trace {
                seed,
                ..trace.clone()
            };
            let mut pilot = RealizePilot::new(arms, target, length.saturating_add(64));
            if self.probe(t, &attempt, &mut pilot).is_ok() && pilot.reached {
                search.found = Some(seed);
                return;
            }
        }
    }

    /// The graph position of `trace`, rebuilt by a replay when the trace changed.
    fn track(&mut self, t: &mut Target<'_>, trace: &Trace) -> Result<Track> {
        let key = TraceKey::of(trace);
        if let Some((cached, track)) = &self.tracked
            && *cached == key
        {
            return Ok(track.clone());
        }
        let artifact = self.artifact.clone().ok_or("no debug artifact")?;
        let graph = artifact.graph.as_ref().ok_or("no story graph")?;
        let mut pilot = ObservePilot {
            tracker: Tracker::new(graph),
        };
        self.probe(t, trace, &mut pilot)?;
        let track = pilot.tracker.track;
        self.tracked = Some((key, track.clone()));
        Ok(track)
    }
}

/// A `reachable` answer for `path`; with draws on it, an empty seed search to fill in.
fn reachable(graph: &StoryGraph, path: &Path) -> NavBody {
    let mut body = NavBody {
        answer: "reachable".to_owned(),
        length: path.length,
        ..NavBody::default()
    };
    let mut search = SeedSearch {
        odds: 1,
        ..SeedSearch::default()
    };
    for action in &path.actions {
        match action {
            Action::Choice { at, arm } => {
                body.path.push(PathItem::Choice {
                    step: graph.step_id(at.context, at.node).unwrap_or_default(),
                    arm: arm.clone(),
                });
                body.push.push(arm.clone());
            }
            Action::Draw {
                at,
                value,
                outcomes,
            } => {
                search.draws = search.draws.saturating_add(1);
                search.odds = search.odds.saturating_mul(u64::from(*outcomes).max(1));
                body.path.push(PathItem::Draw {
                    scene: graph
                        .context(at.context)
                        .map(|c| c.path.clone())
                        .unwrap_or_default(),
                    value: graph.value(*value).map(graph_display).unwrap_or_default(),
                    outcomes: *outcomes,
                });
            }
        }
    }
    body.seed = (search.draws > 0).then_some(search);
    body
}

fn unknown(reason: String) -> NavBody {
    NavBody {
        answer: "unknown".to_owned(),
        reason: Some(reason),
        ..NavBody::default()
    }
}

fn incomplete_reason(graph: &StoryGraph) -> String {
    let reasons: Vec<String> = graph
        .incomplete
        .iter()
        .map(|i| format!("{}: {}", i.code, i.message))
        .collect();
    if reasons.is_empty() {
        "the story graph is incomplete".to_owned()
    } else {
        format!("the story graph is incomplete ({})", reasons.join("; "))
    }
}
