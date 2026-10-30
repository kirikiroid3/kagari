//! Replaying a trace: a save is a witness, re-run from the entry scene through its choices.
//! The same loop drives a debugger's moves ([`crate::debug`]): it follows a trace, then a
//! [`Pilot`] steers the story on, step by step, without side effects.

use std::any::Any;
use std::rc::Rc;

use kagari_vm::{HostObj, HostObject, StoryFiber, StoryStatics, Trap, Value, Vm};

use crate::HostCtx;
use crate::debug::{Arrival, DriveEnd, DriveError, GameView, Pilot, Steer};
use crate::screen::model::ScreenObj;
use crate::screen::runtime::ScreenRuntime;
use crate::story::driver::{self, Resumed};
use crate::story::{Current, GameFrame, Trace};

/// The most steps a replay (or a debugger's drive) runs.
pub(crate) const MAX_STEPS: u32 = 1_000_000;

/// Why a save no longer fits the program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Divergence {
    Empty,
    Unreadable(String),
    Version(i64),
    MissingEntry(String),
    Args(String),
    ChoiceChanged { expected: String, found: String },
    ArmGone { choice: String, arm: String },
    EndedEarly { remaining: usize },
    StepsUnreachable { expected: u32, reached: u32 },
    Panicked(String),
    TooLong,
}

impl Divergence {
    /// The player-facing description.
    pub fn describe(&self) -> String {
        match self {
            Divergence::Empty => "This save slot is empty.".to_owned(),
            Divergence::Unreadable(e) => format!("This save cannot be read: {e}."),
            Divergence::Version(v) => {
                format!("This save has format version {v}, which this game cannot read.")
            }
            Divergence::MissingEntry(p) => {
                format!("The story `{p}` this save was made in no longer exists.")
            }
            Divergence::Args(p) => {
                format!("The starting values of `{p}` no longer fit the story.")
            }
            Divergence::ChoiceChanged { expected, found } => format!(
                "The story no longer reaches the choice `{expected}`: replaying the save met `{found}` first."
            ),
            Divergence::ArmGone { choice, arm } => {
                format!("The choice `{choice}` no longer offers `{arm}`.")
            }
            Divergence::EndedEarly { remaining } => format!(
                "The story now ends before the saved position, with {remaining} saved choices left."
            ),
            Divergence::StepsUnreachable { expected, reached } => format!(
                "The saved position is {expected} lines after the last choice, but the story now has only {reached} there."
            ),
            Divergence::Panicked(m) => {
                format!("The story stopped with an error while replaying the save: {m}")
            }
            Divergence::TooLong => {
                format!("Replaying the save did not reach its position within {MAX_STEPS} steps.")
            }
        }
    }
}

/// The `Divergence` host value.
#[derive(Clone, Debug)]
pub(crate) struct DivergenceObj(pub Divergence);

impl HostObject for DivergenceObj {
    fn clone_object(&self) -> Rc<dyn HostObject> {
        Rc::new(self.clone())
    }
    fn type_name(&self) -> &'static str {
        "Divergence"
    }
    fn eq_object(&self, other: &dyn HostObject) -> Option<bool> {
        let other = other.as_any().downcast_ref::<DivergenceObj>()?;
        Some(self.0.describe() == other.0.describe())
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Replays `trace` into a new game frame; the live game is left as it was.
pub(crate) fn replay(
    ctx: &mut HostCtx,
    vm: &mut Vm,
    trace: &Trace,
    on_exit: Option<Value>,
) -> Result<Box<GameFrame>, Divergence> {
    match drive_trace(ctx, vm, trace, on_exit, &mut StopPilot) {
        Ok((frame, _)) => Ok(frame),
        Err(DriveError::Divergence(d)) => Err(d),
        // Only steering raises these, and `StopPilot` stops at once.
        Err(DriveError::TooLong) => Err(Divergence::TooLong),
        Err(other) => Err(Divergence::Panicked(other.to_string())),
    }
}

/// A pilot that stops at the trace's position.
pub(crate) struct StopPilot;

impl Pilot for StopPilot {
    fn steer(&mut self, _view: &GameView<'_>) -> Steer {
        Steer::Stop
    }
}

/// Replays `trace` into a new game frame and lets `pilot` steer on from its position; the live
/// game is left as it was. Also returns the trace of the last step reached.
pub(crate) fn drive_trace(
    ctx: &mut HostCtx,
    vm: &mut Vm,
    trace: &Trace,
    on_exit: Option<Value>,
    pilot: &mut dyn Pilot,
) -> Result<(Box<GameFrame>, Driven), DriveError> {
    let program = ctx.state.program.clone();
    let entry = program
        .find_function(&trace.entry)
        .filter(|f| {
            program
                .module()
                .functions
                .get(f.0 as usize)
                .is_some_and(|func| func.flags.entry)
        })
        .ok_or_else(|| DriveError::Divergence(Divergence::MissingEntry(trace.entry.clone())))?;
    let fiber = StoryFiber::start(
        vm,
        entry,
        trace.args.clone(),
        StoryStatics::fresh(&program),
        trace.seed,
    )
    .map_err(|_| DriveError::Divergence(Divergence::Args(trace.entry.clone())))?;
    let generation = ctx.state.system.next_generation;
    let fresh = GameFrame::new(
        generation,
        entry,
        fiber,
        ScreenRuntime::new(HostObj::new(ScreenObj::empty())),
        Trace {
            entry: trace.entry.clone(),
            args: trace.args.clone(),
            seed: trace.seed,
            choices: Vec::new(),
            steps_after_last_choice: 0,
        },
        on_exit,
        ctx.now_ms,
    );
    let was_replaying = ctx.state.system.replaying;
    ctx.state.system.replaying = true;
    let live = ctx.state.game.replace(fresh);
    // Each replayed step runs on its own budget, as a load outside any VM call would.
    let result = vm.with_fresh_budget(|vm| run(ctx, vm, Some(trace), pilot));
    let replayed = std::mem::replace(&mut ctx.state.game, live);
    ctx.state.system.replaying = was_replaying;
    let driven = result?;
    let mut frame = replayed.ok_or(DriveError::NoGame)?;
    ctx.state.system.next_generation = generation.saturating_add(1);
    frame.generation = generation;
    Ok((Box::new(frame), driven))
}

/// Lets `pilot` steer the live game on from its current step, in place and without side
/// effects (as during a replay).
pub(crate) fn drive_live(
    ctx: &mut HostCtx,
    vm: &mut Vm,
    pilot: &mut dyn Pilot,
) -> Result<Driven, DriveError> {
    let at_step = ctx
        .state
        .game
        .as_ref()
        .is_some_and(|g| g.fiber.is_some() && !matches!(g.current, Current::None));
    if !at_step {
        return Err(DriveError::NoGame);
    }
    let was_replaying = ctx.state.system.replaying;
    ctx.state.system.replaying = true;
    let result = vm.with_fresh_budget(|vm| run(ctx, vm, None, pilot));
    ctx.state.system.replaying = was_replaying;
    result
}

/// How a drive ended, with the trace of the last step it reached.
pub(crate) struct Driven {
    pub end: DriveEnd,
    pub last: Trace,
}

/// Follows `follow` (its choices, then its step count) from the fiber's start, then steers with
/// `pilot`; without `follow`, steers the game on from its current step.
fn run(
    ctx: &mut HostCtx,
    vm: &mut Vm,
    follow: Option<&Trace>,
    pilot: &mut dyn Pilot,
) -> Result<Driven, DriveError> {
    let diverged = DriveError::Divergence;
    let mut total: u32 = 0;
    let mut steps: u32 = 0;
    let mut choices = follow.map(|t| t.choices.iter());
    let mut next_choice = choices.as_mut().and_then(Iterator::next);
    let mut following = follow.is_some();
    let (mut r, mut arrival) = match follow {
        Some(_) => (
            driver::resume(ctx, vm, Value::unit()).map_err(|t| diverged(panic(&t)))?,
            Arrival::Start,
        ),
        None => (Resumed::Step, Arrival::Current),
    };
    let mut last = live_trace(ctx)?;
    loop {
        total = total.saturating_add(1);
        if total > MAX_STEPS {
            return Err(if following {
                diverged(Divergence::TooLong)
            } else {
                DriveError::TooLong
            });
        }
        if let Resumed::Returned(_) = r {
            if let (true, Some(trace)) = (following, follow) {
                let remaining = usize::from(next_choice.is_some())
                    .saturating_add(choices.as_ref().map_or(0, ExactSizeIterator::len));
                return Err(diverged(if remaining > 0 {
                    Divergence::EndedEarly { remaining }
                } else {
                    Divergence::StepsUnreachable {
                        expected: trace.steps_after_last_choice,
                        reached: steps,
                    }
                }));
            }
            return Ok(Driven {
                end: DriveEnd::Ended,
                last,
            });
        }
        last = live_trace(ctx)?;
        if arrival != Arrival::Current {
            pilot.observe(&GameView::of(ctx.state, vm, &arrival)?);
        }
        if let (true, Some(trace)) = (following, follow) {
            let program = ctx.state.program.clone();
            let game = ctx.state.game.as_ref().ok_or(DriveError::NoGame)?;
            match (&game.current, next_choice) {
                (Current::Choice(choice), Some(saved)) => {
                    let prefix =
                        format!("{}/{}/", program.function_path(choice.step.func), choice.id);
                    let Some(arm_id) = saved.strip_prefix(&prefix) else {
                        return Err(diverged(Divergence::ChoiceChanged {
                            expected: saved.clone(),
                            found: prefix.trim_end_matches('/').to_owned(),
                        }));
                    };
                    let Some(index) = choice
                        .arms
                        .iter()
                        .position(|a| a.shown && a.arm_id == arm_id)
                    else {
                        return Err(diverged(Divergence::ArmGone {
                            choice: prefix.trim_end_matches('/').to_owned(),
                            arm: arm_id.to_owned(),
                        }));
                    };
                    let arm_id = arm_id.to_owned();
                    next_choice = choices.as_mut().and_then(Iterator::next);
                    steps = 0;
                    let index = u32::try_from(index).map_err(|_| diverged(Divergence::TooLong))?;
                    r = driver::select(ctx, vm, index).map_err(|t| diverged(panic(&t)))?;
                    arrival = Arrival::Select(arm_id);
                    continue;
                }
                (Current::Say(_), Some(_)) => {
                    steps = steps.saturating_add(1);
                    r = driver::next_step(ctx, vm).map_err(|t| diverged(panic(&t)))?;
                    arrival = Arrival::Next;
                    continue;
                }
                (_, None) if steps == trace.steps_after_last_choice => following = false,
                (Current::Say(_), None) => {
                    steps = steps.saturating_add(1);
                    r = driver::next_step(ctx, vm).map_err(|t| diverged(panic(&t)))?;
                    arrival = Arrival::Next;
                    continue;
                }
                (_, None) => {
                    return Err(diverged(Divergence::StepsUnreachable {
                        expected: trace.steps_after_last_choice,
                        reached: steps,
                    }));
                }
                (Current::None, Some(_)) => {
                    return Err(diverged(Divergence::EndedEarly {
                        remaining: choices
                            .as_ref()
                            .map_or(0, ExactSizeIterator::len)
                            .saturating_add(1),
                    }));
                }
            }
        }
        let steer = pilot.steer(&GameView::of(ctx.state, vm, &arrival)?);
        let program = ctx.state.program.clone();
        let trapped = |t: Trap| DriveError::Trap(t.render(&program));
        let game = ctx.state.game.as_ref().ok_or(DriveError::NoGame)?;
        match (steer, &game.current) {
            (Steer::Stop, _) => {
                return Ok(Driven {
                    end: DriveEnd::Stopped,
                    last,
                });
            }
            (Steer::Next, Current::Say(_)) => {
                r = driver::next_step(ctx, vm).map_err(trapped)?;
                arrival = Arrival::Next;
            }
            (Steer::Select(index), Current::Choice(choice)) => {
                let Some(arm) = choice.arms.get(index as usize).filter(|a| a.shown) else {
                    return Err(DriveError::Steer(format!(
                        "the choice has no shown arm {index}"
                    )));
                };
                let arm_id = arm.arm_id.clone();
                r = driver::select(ctx, vm, index).map_err(trapped)?;
                arrival = Arrival::Select(arm_id);
            }
            (Steer::Next, _) => {
                return Err(DriveError::Steer(
                    "only a line can go on to the next step; a choice needs an arm".to_owned(),
                ));
            }
            (Steer::Select(_), _) => {
                return Err(DriveError::Steer("only a choice has arms".to_owned()));
            }
        }
    }
}

/// The trace of the game being driven.
fn live_trace(ctx: &HostCtx) -> Result<Trace, DriveError> {
    ctx.state
        .game
        .as_ref()
        .map(|g| g.trace.clone())
        .ok_or(DriveError::NoGame)
}

fn panic(t: &Trap) -> Divergence {
    Divergence::Panicked(first_line(&t.to_string()))
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").to_owned()
}
