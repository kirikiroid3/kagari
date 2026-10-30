//! The query engine: the story commands (`stack`, `ls`, `goto`, `push`, `pop`, `detail`, `nav`,
//! `seed`), step breakpoints and step-level execution, over an [`Engine`]. Every move is an
//! engine drive ([`kagari_engine::debug`]): a replay of a changed trace or the live game
//! steered on, so the debugger and play cannot drift apart.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use kagari_bytecode::Instr;
use kagari_engine::debug::{DriveEnd, GameView, Pilot, Steer, StepId, StepKind, StepView, Trace};
use kagari_engine::{Engine, Platform};
use kagari_vm::{FnKind, Program, Value};

use crate::frames::{Artifact, frame_info, frame_scopes, story_vars};
use crate::hook::HookState;
use crate::nav::{self, Action, Summaries};
use crate::protocol::{
    ArmDetail, ArtifactInfo, BreakpointsBody, Detail, Incomplete, InitializeBody, InstructionPoint,
    LsBody, LsLine, NavBody, PathItem, Position, PushBody, Scope, ScopesBody, SeedBody, SeedSearch,
    StackBody, StoppedBody, Var,
};
use crate::tracker::{Track, Tracker};
use crate::values::{Names, graph_display, plain, rich, to_graph, var};

/// The engine the commands work on, with its platform and the time.
pub struct Target<'a> {
    pub engine: &'a mut Engine,
    pub platform: &'a mut dyn Platform,
    pub now_ms: u64,
}

pub type Result<T, E = String> = std::result::Result<T, E>;

/// How far [`Debugger::run`] goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Run {
    /// To a step breakpoint, a choice, or the end of the story.
    Continue,
    /// To the next step in this scene (or an outer one).
    Next,
    /// To the next step anywhere, a called scene's included.
    StepIn,
    /// To the step after the current scene returns.
    StepOut,
}

/// The seeds `nav` tries by default.
pub const DEFAULT_SEEDS: u32 = 1000;
/// The most lines `ls` lists.
const LS_LIMIT: usize = 10_000;

#[derive(Clone, Debug, PartialEq, Eq)]
struct TraceKey {
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

/// The query engine's state between commands.
pub struct Debugger {
    artifact: Option<Rc<Artifact>>,
    summaries: Option<Rc<Summaries>>,
    tracked: Option<(TraceKey, Track)>,
    step_breakpoints: HashSet<StepId>,
    pub(crate) hook: Rc<RefCell<HookState>>,
}

impl Debugger {
    pub fn new(engine: &Engine, artifact: Option<Rc<Artifact>>) -> Debugger {
        Debugger {
            artifact,
            summaries: None,
            tracked: None,
            step_breakpoints: HashSet::new(),
            hook: Rc::new(RefCell::new(HookState::new(
                engine.program().clone(),
                engine.debug_pause_flag(),
            ))),
        }
    }

    pub fn artifact(&self) -> Option<&Rc<Artifact>> {
        self.artifact.as_ref()
    }

    pub fn initialize(&self, t: &Target<'_>, live: bool) -> InitializeBody {
        InitializeBody {
            script_hash: t.engine.script_hash().to_owned(),
            artifact: self.artifact.as_ref().map(|a| ArtifactInfo {
                graph: a.graph.is_some(),
                complete: a.graph.as_ref().is_some_and(|g| g.complete),
                incomplete: a
                    .graph
                    .as_ref()
                    .map(|g| {
                        g.incomplete
                            .iter()
                            .map(|i| Incomplete {
                                code: i.code.clone(),
                                message: i.message.clone(),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            }),
            live,
            stopped: !live || t.engine.debug_held(),
        }
    }

    /// Runs `f` with the instruction hook off: the debugger's own drives never stop in code.
    fn disarmed<R>(&self, f: impl FnOnce() -> R) -> R {
        let armed = std::mem::replace(&mut self.hook.borrow_mut().armed, false);
        let result = f();
        self.hook.borrow_mut().armed = armed;
        result
    }

    // ----- Position ---------------------------------------------------------------------

    pub fn position(&self, t: &Target<'_>) -> Position {
        match t.engine.debug_view() {
            Some(view) => Position {
                step: view.step.as_ref().map(|s| view.program.step_path(s.step)),
                choices: view.trace.choices.len(),
                steps: view.trace.steps_after_last_choice,
                seed: view.trace.seed,
            },
            None => Position::default(),
        }
    }

    /// A `stopped` event body for the story's current step.
    pub fn stopped(
        &self,
        t: &Target<'_>,
        reason: &str,
        description: Option<String>,
    ) -> StoppedBody {
        StoppedBody {
            reason: reason.to_owned(),
            step: self.position(t).step,
            description,
            in_code: false,
        }
    }

    fn trace(t: &Target<'_>) -> Result<Trace> {
        t.engine
            .debug_view()
            .filter(|v| v.step.is_some())
            .map(|v| v.trace.clone())
            .ok_or_else(|| "no game is running at a step".to_owned())
    }

    // ----- Starting ---------------------------------------------------------------------

    /// Starts the story at an entry scene (the first the story graph knows, or `entry`), with
    /// the graph's arguments, at `seed`.
    pub fn start(&mut self, t: &mut Target<'_>, entry: Option<&str>, seed: u64) -> Result<()> {
        let program = t.engine.program().clone();
        let (path, args) = self.entry(&program, entry)?;
        let trace = Trace {
            entry: path,
            args,
            seed,
            choices: Vec::new(),
            steps_after_last_choice: 0,
        };
        let end = self
            .disarmed(|| t.engine.debug_jump(t.platform, t.now_ms, &trace, &mut Stay))
            .map_err(|e| e.to_string())?;
        if end == DriveEnd::Ended {
            return Err(format!("`{}` returns before its first step", trace.entry));
        }
        Ok(())
    }

    fn entry(&self, program: &Program, entry: Option<&str>) -> Result<(String, Vec<Value>)> {
        if let Some(graph) = self.artifact.as_ref().and_then(|a| a.graph.as_ref()) {
            let root = graph
                .roots
                .iter()
                .filter_map(|r| graph.context(*r))
                .find(|c| entry.is_none_or(|e| c.path == e));
            if let Some(root) = root {
                let args = root
                    .args
                    .iter()
                    .map(|a| {
                        graph
                            .value(*a)
                            .and_then(|v| crate::values::from_graph(program, v))
                    })
                    .collect::<Option<Vec<_>>>()
                    .ok_or_else(|| {
                        format!("the arguments of `{}` do not fit this program", root.path)
                    })?;
                return Ok((root.path.clone(), args));
            }
        }
        let module = program.module();
        let found = module.functions.iter().enumerate().find(|(index, f)| {
            f.flags.entry
                && f.kind == FnKind::Scene
                && entry.is_none_or(|e| {
                    u32::try_from(*index)
                        .is_ok_and(|i| program.function_path(kagari_vm::FuncId(i)) == e)
                })
        });
        let Some((_, function)) = found else {
            return Err(match entry {
                Some(e) => format!("there is no entry scene `{e}`"),
                None => "the program has no entry scene".to_owned(),
            });
        };
        let path = program.string(function.path).to_owned();
        if function.params > 0 {
            return Err(format!(
                "the entry scene `{path}` takes arguments; build with `--profile debug` so the \
                 story graph names them"
            ));
        }
        Ok((path, Vec::new()))
    }

    // ----- Breakpoints ------------------------------------------------------------------

    /// Stop when the story enters one of `steps` (step IDs).
    pub fn set_step_breakpoints(
        &mut self,
        t: &mut Target<'_>,
        steps: &[String],
    ) -> BreakpointsBody {
        let program = t.engine.program().clone();
        let parsed: Vec<Option<StepId>> =
            steps.iter().map(|s| parse_step(&program, s).ok()).collect();
        self.step_breakpoints = parsed.iter().flatten().copied().collect();
        t.engine
            .debug_set_breakpoints(self.step_breakpoints.iter().copied());
        BreakpointsBody {
            verified: parsed.iter().map(Option::is_some).collect(),
        }
    }

    /// Stop before the instructions at these code words.
    pub fn set_instruction_breakpoints(&mut self, points: &[InstructionPoint]) -> BreakpointsBody {
        self.hook.borrow_mut().set_points(points)
    }

    // ----- Stack and variables ----------------------------------------------------------

    /// The story's call stack, innermost first; the innermost frame is at the current step.
    pub fn stack(&self, t: &Target<'_>) -> Result<StackBody> {
        let view = t.engine.debug_view().ok_or("no game is running")?;
        let names = Names {
            program: view.program,
            types: view.types,
        };
        let fiber = view.fiber.ok_or("the story is not at a step")?;
        let frames = fiber.frames();
        let top = frames.len().checked_sub(1);
        let step = view.step.as_ref().map(|s| view.program.step_path(s.step));
        Ok(StackBody {
            frames: frames
                .iter()
                .enumerate()
                .rev()
                .map(|(i, frame)| {
                    let at = if Some(i) == top { step.clone() } else { None };
                    frame_info(self.artifact.as_deref(), names, frame, at)
                })
                .collect(),
        })
    }

    /// The scopes of frame `index` (0 innermost): its arguments and locals, the story
    /// variables, and for the innermost frame the step record.
    pub fn scopes(&self, t: &Target<'_>, index: usize) -> Result<ScopesBody> {
        let view = t.engine.debug_view().ok_or("no game is running")?;
        let names = Names {
            program: view.program,
            types: view.types,
        };
        let fiber = view.fiber.ok_or("the story is not at a step")?;
        let frames = fiber.frames();
        let at = frames
            .len()
            .checked_sub(index.saturating_add(1))
            .and_then(|i| frames.get(i))
            .ok_or("no such frame")?;
        let mut scopes = frame_scopes(self.artifact.as_deref(), names, at);
        scopes.push(story_vars(names, fiber.statics()));
        if index == 0
            && let Some(step) = &view.step
        {
            scopes.push(step_scope(names, &view, step));
        }
        Ok(ScopesBody { scopes })
    }

    // ----- ls, goto, detail ---------------------------------------------------------------

    /// The lines from the current step up to the next choice (included).
    pub fn ls(
        &mut self,
        t: &mut Target<'_>,
        head: Option<u32>,
        tail: Option<u32>,
    ) -> Result<LsBody> {
        let trace = Self::trace(t)?;
        let mut pilot = Segment::new(LS_LIMIT);
        let end = self
            .disarmed(|| {
                t.engine
                    .debug_probe(t.platform, t.now_ms, &trace, &mut pilot)
            })
            .map_err(|e| e.to_string())?;
        let ends = if pilot.hit_limit {
            "limit"
        } else if end == DriveEnd::Ended {
            "end"
        } else {
            "choice"
        };
        let total = pilot.lines.len();
        let mut lines: Vec<LsLine> = pilot.lines.into_iter().map(|s| s.line).collect();
        if let Some(head) = head {
            lines.truncate(head as usize);
        }
        if let Some(tail) = tail {
            let skip = lines.len().saturating_sub(tail as usize);
            lines.drain(..skip);
        }
        Ok(LsBody {
            lines,
            total,
            ends: ends.to_owned(),
        })
    }

    /// The steps between the last choice and the next one: the step after the last choice (or
    /// the story's start) up to the next choice, included.
    fn segment(&self, t: &mut Target<'_>) -> Result<(Trace, Segment)> {
        let mut from = Self::trace(t)?;
        from.steps_after_last_choice = 0;
        let mut pilot = Segment::new(LS_LIMIT);
        self.disarmed(|| {
            t.engine
                .debug_probe(t.platform, t.now_ms, &from, &mut pilot)
        })
        .map_err(|e| e.to_string())?;
        Ok((from, pilot))
    }

    /// Jumps to `step`, which must lie between the last choice and the next one.
    pub fn goto(&mut self, t: &mut Target<'_>, step: &str) -> Result<Position> {
        let program = t.engine.program().clone();
        let target = parse_step(&program, step)?;
        let (mut trace, segment) = self.segment(t)?;
        let Some(found) = segment.lines.iter().find(|s| s.step == target) else {
            let last = trace
                .choices
                .last()
                .and_then(|c| c.rsplit_once('/').map(|(choice, _)| choice.to_owned()));
            let next = segment
                .lines
                .last()
                .filter(|s| s.line.kind == "choice")
                .map(|s| s.line.step.clone());
            return Err(format!(
                "`{step}` is not between the last choice ({}) and the next choice ({}); \
                 use `nav` to find a way there",
                last.map_or_else(|| "the start of the story".to_owned(), |c| format!("`{c}`")),
                next.map_or_else(|| "the end of the story".to_owned(), |c| format!("`{c}`")),
            ));
        };
        trace.steps_after_last_choice = found.steps;
        self.disarmed(|| t.engine.debug_jump(t.platform, t.now_ms, &trace, &mut Stay))
            .map_err(|e| e.to_string())?;
        Ok(self.position(t))
    }

    /// Everything about `step` (default: the current one); another step must lie between the
    /// last choice and the next one.
    pub fn detail(&mut self, t: &mut Target<'_>, step: Option<&str>) -> Result<Detail> {
        let current = t
            .engine
            .debug_view()
            .and_then(|v| v.step.as_ref().map(|s| s.step));
        let program = t.engine.program().clone();
        let target = match step {
            Some(step) => parse_step(&program, step)?,
            None => current.ok_or("no game is running at a step")?,
        };
        if current == Some(target)
            && let Some(view) = t.engine.debug_view()
            && let Some(step) = &view.step
        {
            return Ok(detail_of(&view, step, self.artifact.as_deref()));
        }
        let mut from = Self::trace(t)?;
        from.steps_after_last_choice = 0;
        let mut pilot = DetailPilot {
            target,
            artifact: self.artifact.clone(),
            found: None,
        };
        self.disarmed(|| {
            t.engine
                .debug_probe(t.platform, t.now_ms, &from, &mut pilot)
        })
        .map_err(|e| e.to_string())?;
        pilot.found.ok_or_else(|| {
            format!(
                "`{}` is not between the last choice and the next choice",
                program.step_path(target)
            )
        })
    }

    // ----- push, pop, seed ----------------------------------------------------------------

    /// From the next choice on, selects each arm in turn; stops right after the last one.
    /// Unlike the moves that replay the past (`goto`, `pop`, `seed`), `push` runs story code
    /// that has not run yet, so instruction breakpoints stop in it.
    pub fn push(&mut self, t: &mut Target<'_>, arms: &[String]) -> Result<PushBody> {
        if arms.is_empty() {
            return Err("push needs at least one arm".to_owned());
        }
        Self::trace(t)?;
        let mut pilot = PushPilot {
            arms,
            next: 0,
            selected: Vec::new(),
            error: None,
        };
        let end = t
            .engine
            .debug_step(t.platform, t.now_ms, &mut pilot)
            .map_err(|e| e.to_string());
        self.hook.borrow_mut().clear_step();
        let end = end?;
        if let Some(error) = pilot.error {
            return Err(error);
        }
        if end == DriveEnd::Ended {
            let left = arms.get(pilot.next).map_or("", String::as_str);
            return Err(format!(
                "the story ends before a choice offers `{left}` (stopped at its last step)"
            ));
        }
        Ok(PushBody {
            position: self.position(t),
            selected: pilot.selected,
        })
    }

    /// Back to the last choice, before its arm was selected.
    pub fn pop(&mut self, t: &mut Target<'_>) -> Result<Position> {
        let mut trace = Self::trace(t)?;
        if trace.choices.pop().is_none() {
            return Err("no choice has been made yet".to_owned());
        }
        trace.steps_after_last_choice = 0;
        let mut pilot = Segment::new(LS_LIMIT);
        self.disarmed(|| {
            t.engine
                .debug_jump(t.platform, t.now_ms, &trace, &mut pilot)
        })
        .map_err(|e| e.to_string())?;
        Ok(self.position(t))
    }

    /// The PRNG seed; with `seed`, replays the trace with it.
    pub fn seed(&mut self, t: &mut Target<'_>, seed: Option<u64>) -> Result<SeedBody> {
        let mut trace = Self::trace(t)?;
        if let Some(seed) = seed {
            trace.seed = seed;
            self.disarmed(|| t.engine.debug_jump(t.platform, t.now_ms, &trace, &mut Stay))
                .map_err(|e| e.to_string())?;
        }
        Ok(SeedBody { seed: trace.seed })
    }

    // ----- nav --------------------------------------------------------------------------

    /// Whether `step` can be reached from the current state, and one shortest way.
    pub fn nav(&mut self, t: &mut Target<'_>, step: &str, seeds: Option<u32>) -> Result<NavBody> {
        let program = t.engine.program().clone();
        let target = parse_step(&program, step)?;
        let target_name = program.step_path(target);
        let artifact = self
            .artifact
            .clone()
            .ok_or("`nav` needs the debug artifact: build with `kagari build --profile debug`")?;
        let graph = artifact
            .graph
            .as_ref()
            .ok_or("the debug artifact has no story graph")?;
        let trace = Self::trace(t)?;
        let track = self.track(t, &trace)?;
        let real = match track {
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
        let sums = match &self.summaries {
            Some(sums) => sums.clone(),
            None => {
                let sums = Rc::new(Summaries::of(graph));
                self.summaries = Some(sums.clone());
                sums
            }
        };
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
        let mut body = NavBody {
            answer: "reachable".to_owned(),
            length: path.length,
            ..NavBody::default()
        };
        let mut draws: u32 = 0;
        let mut odds: u64 = 1;
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
                    draws = draws.saturating_add(1);
                    odds = odds.saturating_mul(u64::from(*outcomes).max(1));
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
        if draws > 0 {
            let limit = seeds.unwrap_or(DEFAULT_SEEDS);
            let mut search = SeedSearch {
                draws,
                odds,
                tried: 0,
                limit,
                current: trace.seed,
                found: None,
            };
            let budget = path.length.saturating_add(64);
            // The current seed first: the draws may already come out this way.
            let seeds = std::iter::once(trace.seed)
                .chain((0..u64::MAX).filter(|s| *s != trace.seed))
                .take(limit as usize);
            for seed in seeds {
                search.tried = search.tried.saturating_add(1);
                let mut attempt = trace.clone();
                attempt.seed = seed;
                let mut pilot = RealizePilot {
                    arms: &body.push,
                    next: 0,
                    target,
                    budget,
                    reached: false,
                };
                let result = self.disarmed(|| {
                    t.engine
                        .debug_probe(t.platform, t.now_ms, &attempt, &mut pilot)
                });
                if result.is_ok() && pilot.reached {
                    search.found = Some(seed);
                    break;
                }
            }
            body.seed = Some(search);
        }
        Ok(body)
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
        self.disarmed(|| {
            t.engine
                .debug_probe(t.platform, t.now_ms, trace, &mut pilot)
        })
        .map_err(|e| e.to_string())?;
        let track = pilot.tracker.track;
        self.tracked = Some((key, track.clone()));
        Ok(track)
    }

    // ----- Execution ----------------------------------------------------------------------

    /// Drives the story on from its current step (`continue`, `next`, `stepIn`, `stepOut`),
    /// with side effects off, and installs it where it stops. `interrupted` is asked at every
    /// step whether a pause was requested.
    pub fn run(
        &mut self,
        t: &mut Target<'_>,
        how: Run,
        interrupted: &mut dyn FnMut() -> bool,
    ) -> Result<StoppedBody> {
        let view = t.engine.debug_view().ok_or("no game is running")?;
        let step = view.step.as_ref().ok_or("the story is not at a step")?;
        if let StepKind::Choice { id, .. } = &step.kind {
            return Err(format!(
                "the story waits at the choice `{}/{id}`: select an arm with `push`",
                view.program.function_path(step.step.func)
            ));
        }
        let depth = view.fiber.map_or(0, |f| f.frames().len());
        let mut pilot = ExecPilot {
            how,
            depth,
            first: true,
            breakpoints: &self.step_breakpoints,
            hook: &self.hook,
            interrupted,
            reason: None,
        };
        let end = t
            .engine
            .debug_step(t.platform, t.now_ms, &mut pilot)
            .map_err(|e| e.to_string())?;
        let reason = pilot.reason;
        self.hook.borrow_mut().clear_step();
        Ok(match end {
            DriveEnd::Ended => self.stopped(
                t,
                "end",
                Some("the story returns after this step".to_owned()),
            ),
            DriveEnd::Stopped => self.stopped(t, reason.unwrap_or("step"), None),
        })
    }
}

fn unknown(reason: String) -> NavBody {
    NavBody {
        answer: "unknown".to_owned(),
        reason: Some(reason),
        ..NavBody::default()
    }
}

fn incomplete_reason(graph: &kagari_debug::StoryGraph) -> String {
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

/// `{function path}#{n}` as a step of this program.
pub fn parse_step(program: &Program, step: &str) -> Result<StepId> {
    let (path, id) = step
        .rsplit_once('#')
        .ok_or_else(|| format!("`{step}` is not a step ID (`path#n`)"))?;
    let id: u32 = id
        .parse()
        .map_err(|_| format!("`{step}` is not a step ID (`path#n`)"))?;
    let func = program
        .find_function(path)
        .ok_or_else(|| format!("there is no function `{path}`"))?;
    let exists = program
        .module()
        .functions
        .get(func.0 as usize)
        .is_some_and(|f| {
            f.code
                .iter()
                .any(|i| matches!(i, Instr::Step { id: s, .. } if *s == id))
        });
    if !exists {
        return Err(format!("`{path}` has no step {id}"));
    }
    Ok(StepId { func, id })
}

/// The step record as a scope.
fn step_scope(names: Names<'_>, view: &GameView<'_>, step: &StepView<'_>) -> Scope {
    let mut vars = vec![Var {
        name: "step".to_owned(),
        value: view.program.step_path(step.step),
        children: Vec::new(),
    }];
    if let Some(render) = step.render {
        vars.push(var(names, "render", render));
    }
    match &step.kind {
        StepKind::Say { who, text, voice } => {
            if let Some(who) = who {
                vars.push(text_var("who", names, who));
            }
            if let Some(text) = text {
                vars.push(text_var("text", names, text));
            }
            if let Some(voice) = voice {
                vars.push(Var {
                    name: "voice".to_owned(),
                    value: view.asset_path(*voice).to_owned(),
                    children: Vec::new(),
                });
            }
        }
        StepKind::Choice { id, arms } => {
            vars.push(Var {
                name: "id".to_owned(),
                value: (*id).to_owned(),
                children: Vec::new(),
            });
            vars.push(Var {
                name: "arms".to_owned(),
                value: format!("{} arms", arms.len()),
                children: arms
                    .iter()
                    .map(|arm| Var {
                        name: arm.arm_id.to_owned(),
                        value: format!(
                            "{}{}",
                            plain(arm.text),
                            if arm.shown { "" } else { " (hidden)" }
                        ),
                        children: Vec::new(),
                    })
                    .collect(),
            });
        }
    }
    Scope {
        name: "Step".to_owned(),
        vars,
    }
}

fn text_var(name: &str, names: Names<'_>, text: &kagari_vm::Text) -> Var {
    Var {
        name: name.to_owned(),
        value: rich(names, text),
        children: Vec::new(),
    }
}

fn detail_of(view: &GameView<'_>, step: &StepView<'_>, artifact: Option<&Artifact>) -> Detail {
    let names = Names {
        program: view.program,
        types: view.types,
    };
    let mut detail = Detail {
        step: view.program.step_path(step.step),
        render: step.render.map(|r| crate::values::display(names, r)),
        keyframe: keyframe(artifact, names, step.render),
        ..Detail::default()
    };
    match &step.kind {
        StepKind::Say { who, text, voice } => {
            detail.kind = "say".to_owned();
            detail.who = who.map(|w| rich(names, w));
            detail.text = text.map(|t| rich(names, t));
            detail.voice = voice.map(|v| view.asset_path(v).to_owned());
        }
        StepKind::Choice { id, arms } => {
            detail.kind = "choice".to_owned();
            detail.choice = Some((*id).to_owned());
            detail.arms = arms
                .iter()
                .map(|arm| ArmDetail {
                    arm: arm.arm_id.to_owned(),
                    text: rich(names, arm.text),
                    shown: arm.shown,
                })
                .collect();
        }
    }
    detail
}

/// Whether the render is a keyframe, as the story graph knows it; a step without a render is
/// not one.
fn keyframe(artifact: Option<&Artifact>, names: Names<'_>, render: Option<&Value>) -> Option<bool> {
    let graph = artifact?.graph.as_ref()?;
    let Some(render) = render else {
        return Some(false);
    };
    let render = to_graph(names, render)?;
    let index = graph.values.iter().position(|v| *v == render)?;
    Some(graph.is_keyframe(u32::try_from(index).ok()?))
}

fn ls_line(view: &GameView<'_>, step: &StepView<'_>) -> LsLine {
    let program = view.program;
    match &step.kind {
        StepKind::Say { who, text, .. } => LsLine {
            step: program.step_path(step.step),
            kind: "say".to_owned(),
            who: who.map(plain),
            text: text.map(plain).unwrap_or_default(),
        },
        StepKind::Choice { id, .. } => LsLine {
            step: program.step_path(step.step),
            kind: "choice".to_owned(),
            who: None,
            text: (*id).to_owned(),
        },
    }
}

// ----- Pilots ---------------------------------------------------------------------------

/// Stays at the trace's position.
struct Stay;

impl Pilot for Stay {
    fn steer(&mut self, _view: &GameView<'_>) -> Steer {
        Steer::Stop
    }
}

/// A step of a segment: its ID, its `ls` line, and the trace's line count there.
struct SegmentStep {
    step: StepId,
    line: LsLine,
    steps: u32,
}

/// Goes on through lines and stops at the next choice, recording each step.
struct Segment {
    lines: Vec<SegmentStep>,
    limit: usize,
    hit_limit: bool,
}

impl Segment {
    fn new(limit: usize) -> Segment {
        Segment {
            lines: Vec::new(),
            limit,
            hit_limit: false,
        }
    }
}

impl Pilot for Segment {
    fn steer(&mut self, view: &GameView<'_>) -> Steer {
        let Some(step) = &view.step else {
            return Steer::Stop;
        };
        self.lines.push(SegmentStep {
            step: step.step,
            line: ls_line(view, step),
            steps: view.trace.steps_after_last_choice,
        });
        match step.kind {
            StepKind::Choice { .. } => Steer::Stop,
            StepKind::Say { .. } if self.lines.len() >= self.limit => {
                self.hit_limit = true;
                Steer::Stop
            }
            StepKind::Say { .. } => Steer::Next,
        }
    }
}

/// Goes on to `target` within the segment and takes its detail.
struct DetailPilot {
    target: StepId,
    artifact: Option<Rc<Artifact>>,
    found: Option<Detail>,
}

impl Pilot for DetailPilot {
    fn steer(&mut self, view: &GameView<'_>) -> Steer {
        let Some(step) = &view.step else {
            return Steer::Stop;
        };
        if step.step == self.target {
            self.found = Some(detail_of(view, step, self.artifact.as_deref()));
            return Steer::Stop;
        }
        match step.kind {
            StepKind::Choice { .. } => Steer::Stop,
            StepKind::Say { .. } => Steer::Next,
        }
    }
}

/// Whether the arm `spec` names this arm: its arm ID, `#index`, or the last segments of the
/// ID (`B`, `Pick::B` for `crate::Pick::B`).
fn arm_matches(spec: &str, arm_id: &str, index: usize) -> bool {
    arm_id == spec
        || spec.strip_prefix('#').and_then(|i| i.parse::<usize>().ok()) == Some(index)
        || arm_id
            .strip_suffix(spec)
            .is_some_and(|rest| rest.ends_with("::"))
}

/// Selects the arms in turn, going on through lines; stops at the step after the last one.
struct PushPilot<'a> {
    arms: &'a [String],
    next: usize,
    selected: Vec<String>,
    error: Option<String>,
}

impl Pilot for PushPilot<'_> {
    fn steer(&mut self, view: &GameView<'_>) -> Steer {
        let Some(spec) = self.arms.get(self.next) else {
            return Steer::Stop;
        };
        let Some(step) = &view.step else {
            return Steer::Stop;
        };
        let StepKind::Choice { id, arms } = &step.kind else {
            return Steer::Next;
        };
        let choice = format!("{}/{id}", view.program.function_path(step.step.func));
        let shown: Vec<(usize, &str)> = arms
            .iter()
            .enumerate()
            .filter(|(_, a)| a.shown)
            .map(|(i, a)| (i, a.arm_id))
            .collect();
        let matching: Vec<&(usize, &str)> = shown
            .iter()
            .filter(|(i, arm)| arm_matches(spec, arm, *i))
            .collect();
        let (index, arm) = match matching.as_slice() {
            [one] => **one,
            [] => {
                let offered: Vec<String> = shown.iter().map(|(_, a)| format!("`{a}`")).collect();
                self.error = Some(format!(
                    "the choice `{choice}` does not offer `{spec}` (it offers {})",
                    offered.join(", ")
                ));
                return Steer::Stop;
            }
            _ => {
                self.error = Some(format!(
                    "`{spec}` names more than one arm of the choice `{choice}`"
                ));
                return Steer::Stop;
            }
        };
        self.selected.push(format!("{choice}/{arm}"));
        self.next = self.next.saturating_add(1);
        match u32::try_from(index) {
            Ok(index) => Steer::Select(index),
            Err(_) => Steer::Stop,
        }
    }
}

/// Feeds every step to a tracker.
struct ObservePilot<'g> {
    tracker: Tracker<'g>,
}

impl Pilot for ObservePilot<'_> {
    fn observe(&mut self, view: &GameView<'_>) {
        self.tracker.observe(view);
    }

    fn steer(&mut self, _view: &GameView<'_>) -> Steer {
        Steer::Stop
    }
}

/// Follows a `nav` path's arms and checks that `target` is reached.
struct RealizePilot<'a> {
    arms: &'a [String],
    next: usize,
    target: StepId,
    budget: u32,
    reached: bool,
}

impl Pilot for RealizePilot<'_> {
    fn steer(&mut self, view: &GameView<'_>) -> Steer {
        let Some(step) = &view.step else {
            return Steer::Stop;
        };
        if step.step == self.target {
            self.reached = true;
            return Steer::Stop;
        }
        let Some(budget) = self.budget.checked_sub(1) else {
            return Steer::Stop;
        };
        self.budget = budget;
        match &step.kind {
            StepKind::Say { .. } => Steer::Next,
            StepKind::Choice { arms, .. } => {
                let Some(spec) = self.arms.get(self.next) else {
                    return Steer::Stop;
                };
                let index = arms.iter().position(|a| a.shown && a.arm_id == spec);
                self.next = self.next.saturating_add(1);
                match index.and_then(|i| u32::try_from(i).ok()) {
                    Some(index) => Steer::Select(index),
                    None => Steer::Stop,
                }
            }
        }
    }
}

/// `continue`, `next`, `stepIn`, `stepOut`.
struct ExecPilot<'a> {
    how: Run,
    /// The fiber's depth where the run started.
    depth: usize,
    first: bool,
    breakpoints: &'a HashSet<StepId>,
    hook: &'a Rc<RefCell<HookState>>,
    interrupted: &'a mut dyn FnMut() -> bool,
    reason: Option<&'static str>,
}

impl Pilot for ExecPilot<'_> {
    fn steer(&mut self, view: &GameView<'_>) -> Steer {
        if std::mem::replace(&mut self.first, false) {
            return Steer::Next;
        }
        let Some(step) = &view.step else {
            return Steer::Stop;
        };
        let depth = view.fiber.map_or(0, |f| f.frames().len());
        let stop = |reason: &'static str, this: &mut Self| {
            this.reason = Some(reason);
            Steer::Stop
        };
        // A code step that reached a step ends here.
        let code_step = self.hook.borrow().story_step();
        if code_step {
            return stop("step", self);
        }
        if self.breakpoints.contains(&step.step) {
            return stop("breakpoint", self);
        }
        let stepped = match self.how {
            Run::Continue => false,
            Run::StepIn => true,
            Run::Next => depth <= self.depth,
            Run::StepOut => depth < self.depth,
        };
        if stepped {
            return stop("step", self);
        }
        if matches!(step.kind, StepKind::Choice { .. }) {
            return stop("choice", self);
        }
        if (self.interrupted)() {
            return stop("pause", self);
        }
        Steer::Next
    }
}
