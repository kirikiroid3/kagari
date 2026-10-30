//! The query engine: the story commands (`stack`, `ls`, `goto`, `push`, `pop`, `detail`, `nav`,
//! `seed`), step breakpoints and step-level execution, over an [`Engine`]. Every move is an
//! engine drive ([`kagari_engine::debug`]): a replay of a changed trace or the live game
//! steered on, so the debugger and play cannot drift apart.
//!
//! This module holds the state, starting, breakpoints, the stack and scopes; the moves are in
//! `moves.rs`, `nav` in `navigate.rs`, execution in `exec.rs`, the pilots that steer the drives
//! in `pilots.rs` and the step views in `views.rs`.

mod exec;
mod moves;
mod navigate;
mod pilots;
mod views;

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use kagari_bytecode::Instr;
use kagari_engine::debug::{DriveEnd, Pilot, StepId, Trace};
use kagari_engine::{Engine, Platform};
use kagari_vm::{FnKind, Program, Value};

use crate::frames::{Artifact, frame_info, frame_scopes, story_vars};
use crate::hook::HookState;
use crate::nav::Summaries;
use crate::protocol::{
    ArtifactInfo, BreakpointsBody, Incomplete, InitializeBody, InstructionPoint, Position,
    ScopesBody, StackBody, StoppedBody,
};
use crate::tracker::Track;
use crate::values::Names;
use pilots::Stay;

/// The engine the commands work on, with its platform and the time.
pub struct Target<'a> {
    pub engine: &'a mut Engine,
    pub platform: &'a mut dyn Platform,
    pub now_ms: u64,
}

pub type Result<T, E = String> = std::result::Result<T, E>;

pub use exec::Run;
pub use navigate::DEFAULT_SEEDS;
use navigate::TraceKey;
use views::{detail_of, step_scope};

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

    /// Replays `trace` into a scratch game steered by `pilot` (hook off).
    fn probe(&self, t: &mut Target<'_>, trace: &Trace, pilot: &mut dyn Pilot) -> Result<DriveEnd> {
        self.disarmed(|| t.engine.debug_probe(t.platform, t.now_ms, trace, pilot))
            .map_err(|e| e.to_string())
    }

    /// Replays `trace`, steers on with `pilot` and installs the game (hook off).
    fn jump(&self, t: &mut Target<'_>, trace: &Trace, pilot: &mut dyn Pilot) -> Result<DriveEnd> {
        self.disarmed(|| t.engine.debug_jump(t.platform, t.now_ms, trace, pilot))
            .map_err(|e| e.to_string())
    }

    /// Steers the live game on with `pilot` (hook armed) and ends any code step.
    fn step(&self, t: &mut Target<'_>, pilot: &mut dyn Pilot) -> Result<DriveEnd> {
        let end = t.engine.debug_step(t.platform, t.now_ms, pilot);
        self.hook.borrow_mut().clear_step();
        end.map_err(|e| e.to_string())
    }

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
        let end = self.jump(t, &trace, &mut Stay)?;
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
            scopes.push(step_scope(detail_of(&view, step, self.artifact.as_deref())));
        }
        Ok(ScopesBody { scopes })
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
