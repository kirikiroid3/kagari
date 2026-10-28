//! Replaying a trace: a save is a witness, re-run from the entry scene through its choices.

use std::any::Any;
use std::rc::Rc;

use kagari_vm::{HostObj, HostObject, StoryFiber, StoryStatics, Value, Vm};

use crate::HostCtx;
use crate::screen::model::ScreenObj;
use crate::screen::runtime::ScreenRuntime;
use crate::story::driver::{self, Resumed};
use crate::story::{Current, GameFrame, Trace};

/// The most steps a replay runs.
const MAX_STEPS: u32 = 1_000_000;

/// Why a save no longer fits the program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Divergence {
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
        .ok_or_else(|| Divergence::MissingEntry(trace.entry.clone()))?;
    let fiber = StoryFiber::start(
        vm,
        entry,
        trace.args.clone(),
        StoryStatics::fresh(&program),
        trace.seed,
    )
    .map_err(|_| Divergence::Args(trace.entry.clone()))?;
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
    let result = vm.with_fresh_budget(|vm| run(ctx, vm, trace));
    let replayed = std::mem::replace(&mut ctx.state.game, live);
    ctx.state.system.replaying = was_replaying;
    result?;
    let mut frame = replayed.ok_or(Divergence::TooLong)?;
    ctx.state.system.next_generation = generation.saturating_add(1);
    frame.generation = generation;
    Ok(Box::new(frame))
}

fn run(ctx: &mut HostCtx, vm: &mut Vm, trace: &Trace) -> Result<(), Divergence> {
    let mut total: u32 = 0;
    let mut steps: u32 = 0;
    let mut choices = trace.choices.iter();
    let mut next_choice = choices.next();
    let mut r = driver::resume(ctx, vm, Value::unit()).map_err(|t| panic(&t))?;
    loop {
        total = total.saturating_add(1);
        if total > MAX_STEPS {
            return Err(Divergence::TooLong);
        }
        if let Resumed::Returned(_) = r {
            let remaining = usize::from(next_choice.is_some()).saturating_add(choices.len());
            return Err(if remaining > 0 {
                Divergence::EndedEarly { remaining }
            } else {
                Divergence::StepsUnreachable {
                    expected: trace.steps_after_last_choice,
                    reached: steps,
                }
            });
        }
        let program = ctx.state.program.clone();
        let game = ctx.state.game.as_ref().ok_or(Divergence::TooLong)?;
        match (&game.current, next_choice) {
            (Current::Choice(choice), Some(saved)) => {
                let prefix = format!("{}/{}/", program.function_path(choice.func), choice.id);
                let Some(arm_id) = saved.strip_prefix(&prefix) else {
                    return Err(Divergence::ChoiceChanged {
                        expected: saved.clone(),
                        found: prefix.trim_end_matches('/').to_owned(),
                    });
                };
                let Some(index) = choice
                    .arms
                    .iter()
                    .position(|a| a.shown && a.arm_id == arm_id)
                else {
                    return Err(Divergence::ArmGone {
                        choice: prefix.trim_end_matches('/').to_owned(),
                        arm: arm_id.to_owned(),
                    });
                };
                next_choice = choices.next();
                steps = 0;
                let index = u32::try_from(index).map_err(|_| Divergence::TooLong)?;
                r = driver::select(ctx, vm, index).map_err(|t| panic(&t))?;
            }
            (Current::Say(_), Some(_)) => {
                steps = steps.saturating_add(1);
                r = driver::next_step(ctx, vm).map_err(|t| panic(&t))?;
            }
            (_, None) if steps == trace.steps_after_last_choice => return Ok(()),
            (Current::Say(_), None) => {
                steps = steps.saturating_add(1);
                r = driver::next_step(ctx, vm).map_err(|t| panic(&t))?;
            }
            (_, None) => {
                return Err(Divergence::StepsUnreachable {
                    expected: trace.steps_after_last_choice,
                    reached: steps,
                });
            }
            (Current::None, Some(_)) => {
                return Err(Divergence::EndedEarly {
                    remaining: choices.len().saturating_add(1),
                });
            }
        }
    }
}

fn panic(t: &kagari_vm::Trap) -> Divergence {
    Divergence::Panicked(first_line(&t.to_string()))
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").to_owned()
}
