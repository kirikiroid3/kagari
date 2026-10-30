//! What a debugger needs of the engine (`kagari-debugger`, see
//! `docs/dev/src/runtime/debugger.md`).
//!
//! - **Positions are traces.** The story's position is its [`Trace`], as a save records it. A
//!   debugger moves the story by driving it: the same loop that replays a save follows a trace,
//!   then a [`Pilot`] steers on step by step. Drives run with side effects off, like a replay.
//! - **Probing** ([`Engine::debug_probe`]) drives a scratch copy and throws it away (lookahead,
//!   seed search); **jumping** ([`Engine::debug_jump`]) and **stepping**
//!   ([`Engine::debug_step`]) install the result the way a load does (`NavOp::Restore`), so the
//!   screen shows the new step at once, without the animations in between.
//! - **Holding.** After a debugger move, or when the live game enters a step breakpoint, the
//!   game is held: the player's story input, auto and skip are dropped until
//!   [`Engine::debug_release`]; the system UI keeps running. The hold belongs to one game
//!   (its generation), so a load or a new game by the player ends it.

use std::collections::HashSet;
use std::fmt;
use std::rc::Rc;

use kagari_vm::{AssetId, InstrHook, Program, StoryFiber, Text, Trap, TypeTable, Value, Vm};

use crate::saves::replay::{self, StopPilot};
use crate::story::{Current, GameFrame};
use crate::system::NavOp;
use crate::{Engine, EngineState, HostCtx, Platform, system};

pub use crate::saves::replay::Divergence;
pub use crate::story::Trace;
pub use kagari_vm::StepId;

/// How a drive reached a step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Arrival {
    /// The entry scene's first step.
    Start,
    /// After a line.
    Next,
    /// After selecting the arm with this arm ID.
    Select(String),
    /// The step the game was already at when the drive began.
    Current,
}

/// What a pilot does at a step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Steer {
    /// Go on from a line.
    Next,
    /// Select arm `index` (in the choice's arm list) of a choice.
    Select(u32),
    /// Stay at this step.
    Stop,
}

/// Steers a drive.
pub trait Pilot {
    /// Every step the drive enters, in order, the steps of the followed trace included (not
    /// the step a live drive starts from).
    fn observe(&mut self, _view: &GameView<'_>) {}
    /// At the followed trace's position, then at every step after it: what to do.
    fn steer(&mut self, view: &GameView<'_>) -> Steer;
}

/// How a drive ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriveEnd {
    /// The pilot stopped at a step.
    Stopped,
    /// The entry scene returned while steering. Nothing is installed: the game stays where it
    /// was (a live drive goes back to the last step it reached).
    Ended,
}

/// Why a drive failed. The live game is unchanged (a failed live drive is rebuilt from the
/// trace it started at).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DriveError {
    /// No game is running, or it is not at a step.
    NoGame,
    /// The game has crashed or quit.
    Stopped,
    /// The trace no longer fits the program.
    Divergence(Divergence),
    /// The story trapped while steering (rendered with its location).
    Trap(String),
    /// The pilot asked for something the step cannot do.
    Steer(String),
    /// More than the replay limit of steps.
    TooLong,
}

impl fmt::Display for DriveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriveError::NoGame => f.write_str("no game is running at a step"),
            DriveError::Stopped => f.write_str("the game has stopped"),
            DriveError::Divergence(d) => f.write_str(&d.describe()),
            DriveError::Trap(message) => write!(f, "the story stopped with an error: {message}"),
            DriveError::Steer(message) => f.write_str(message),
            DriveError::TooLong => write!(
                f,
                "the story did not stop within {} steps",
                replay::MAX_STEPS
            ),
        }
    }
}

impl std::error::Error for DriveError {}

/// The game at a step, as a pilot or a debugger sees it.
pub struct GameView<'a> {
    pub program: &'a Program,
    pub types: &'a TypeTable,
    /// The position.
    pub trace: &'a Trace,
    pub arrival: &'a Arrival,
    /// `None` once the story returned.
    pub step: Option<StepView<'a>>,
    /// The story fiber: frames, story variables, the draws of the last resume.
    pub fiber: Option<&'a StoryFiber>,
    /// The game's number; a load or a new game changes it.
    pub generation: u64,
    assets: &'a crate::AssetStore,
}

/// A step the game is at.
pub struct StepView<'a> {
    pub step: StepId,
    /// The render function value the step record names.
    pub render: Option<&'a Value>,
    pub kind: StepKind<'a>,
}

pub enum StepKind<'a> {
    Say {
        who: Option<&'a Text>,
        text: Option<&'a Text>,
        voice: Option<AssetId>,
    },
    Choice {
        /// The choice's `id`, flattened to plain characters.
        id: &'a str,
        /// Every arm, shown or not, in list order.
        arms: Vec<ArmView<'a>>,
    },
}

pub struct ArmView<'a> {
    /// The arm ID: the value's variant path, or `#index`.
    pub arm_id: &'a str,
    pub text: &'a Text,
    pub shown: bool,
    pub value: &'a Value,
}

impl<'a> GameView<'a> {
    pub(crate) fn of(
        state: &'a EngineState,
        vm: &'a Vm,
        arrival: &'a Arrival,
    ) -> Result<GameView<'a>, DriveError> {
        let game = state.game.as_ref().ok_or(DriveError::NoGame)?;
        Ok(GameView {
            program: &state.program,
            types: vm.types(),
            trace: &game.trace,
            arrival,
            step: step_view(game),
            fiber: game.fiber.as_ref(),
            generation: game.generation,
            assets: &state.assets,
        })
    }

    /// The bytecode path of an asset.
    pub fn asset_path(&self, id: AssetId) -> &'a str {
        self.assets.path(id)
    }
}

fn step_view(game: &GameFrame) -> Option<StepView<'_>> {
    let render = game.render.as_ref();
    match &game.current {
        Current::None => None,
        Current::Say(say) => Some(StepView {
            step: say.step,
            render,
            kind: StepKind::Say {
                who: say.who.as_ref(),
                text: say.text.as_ref(),
                voice: say.voice,
            },
        }),
        Current::Choice(choice) => Some(StepView {
            step: choice.step,
            render,
            kind: StepKind::Choice {
                id: &choice.id,
                arms: choice
                    .arms
                    .iter()
                    .map(|arm| ArmView {
                        arm_id: &arm.arm_id,
                        text: &arm.text,
                        shown: arm.shown,
                        value: &arm.value,
                    })
                    .collect(),
            },
        }),
    }
}

/// Why the live game stopped by itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    /// It entered a step breakpoint.
    Breakpoint,
    /// A pause was requested.
    Pause,
}

/// The live game stopped at `step` and is held.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiveStop {
    pub step: StepId,
    pub reason: StopReason,
}

/// The debugger's hold on the live game.
#[derive(Default)]
pub(crate) struct DebugState {
    /// The generation of the held game.
    held: Option<u64>,
    /// Stop at the next step the live game enters.
    pause: bool,
    breakpoints: HashSet<StepId>,
    /// A stop not yet taken by the debugger.
    stop: Option<LiveStop>,
}

/// Whether the debugger holds the running game.
pub(crate) fn held(state: &EngineState) -> bool {
    state.debug.held.is_some() && state.game.as_ref().map(|g| g.generation) == state.debug.held
}

/// The live game (not a replay) entered `step`: stop at a breakpoint or a requested pause.
pub(crate) fn entered_live(state: &mut EngineState, step: StepId) {
    let reason = if state.debug.breakpoints.contains(&step) {
        StopReason::Breakpoint
    } else if state.debug.pause {
        StopReason::Pause
    } else {
        return;
    };
    let Some(game) = state.game.as_ref() else {
        return;
    };
    state.debug.held = Some(game.generation);
    state.debug.pause = false;
    state.debug.stop = Some(LiveStop { step, reason });
}

static CURRENT: Arrival = Arrival::Current;

impl Engine {
    /// The loaded program.
    pub fn program(&self) -> &Rc<Program> {
        &self.state.program
    }

    /// The type table (paths and variant names of the values the VM builds).
    pub fn types(&self) -> &TypeTable {
        self.vm.types()
    }

    /// The lowercase hex SHA-256 of the `game.kagari` this engine runs.
    pub fn script_hash(&self) -> &str {
        &self.state.system.script_hash
    }

    /// The running game at its current step; `None` without a game.
    pub fn debug_view(&self) -> Option<GameView<'_>> {
        GameView::of(&self.state, &self.vm, &CURRENT).ok()
    }

    /// Replays `trace` into a scratch game and lets `pilot` steer on; nothing is installed and
    /// nothing persistent or audible happens.
    pub fn debug_probe(
        &mut self,
        platform: &mut dyn Platform,
        now_ms: u64,
        trace: &Trace,
        pilot: &mut dyn Pilot,
    ) -> Result<DriveEnd, DriveError> {
        self.ensure_running()?;
        let (mut ctx, vm) = self.parts(platform, now_ms);
        let on_exit = same_entry_exit(&ctx, trace);
        let (_, driven) = replay::drive_trace(&mut ctx, vm, trace, on_exit, pilot)?;
        Ok(driven.end)
    }

    /// Replays `trace`, lets `pilot` steer on, and installs the game where the pilot stopped,
    /// held. When the story ends while steering, nothing changes.
    pub fn debug_jump(
        &mut self,
        platform: &mut dyn Platform,
        now_ms: u64,
        trace: &Trace,
        pilot: &mut dyn Pilot,
    ) -> Result<DriveEnd, DriveError> {
        self.ensure_running()?;
        self.now_ms = now_ms;
        let (mut ctx, vm) = self.parts(platform, now_ms);
        let on_exit = same_entry_exit(&ctx, trace);
        let (frame, driven) = replay::drive_trace(&mut ctx, vm, trace, on_exit, pilot)?;
        if driven.end == DriveEnd::Stopped {
            install(&mut ctx, vm, frame);
        }
        Ok(driven.end)
    }

    /// Lets `pilot` steer the live game on from its current step, then installs it where the
    /// pilot stopped, held. When the story ends, the game goes back to the last step reached;
    /// when the drive fails, to the step it started at.
    pub fn debug_step(
        &mut self,
        platform: &mut dyn Platform,
        now_ms: u64,
        pilot: &mut dyn Pilot,
    ) -> Result<DriveEnd, DriveError> {
        self.ensure_running()?;
        self.now_ms = now_ms;
        let (mut ctx, vm) = self.parts(platform, now_ms);
        let (before, on_exit) = ctx
            .state
            .game
            .as_ref()
            .map(|g| (g.trace.clone(), g.on_exit.clone()))
            .ok_or(DriveError::NoGame)?;
        match replay::drive_live(&mut ctx, vm, pilot) {
            Ok(driven) if driven.end == DriveEnd::Stopped => {
                if let Some(frame) = ctx.state.game.take() {
                    ctx.state.audio.end_game();
                    install(&mut ctx, vm, Box::new(frame));
                }
                Ok(DriveEnd::Stopped)
            }
            Ok(driven) => {
                rebuild(&mut ctx, vm, &driven.last, on_exit);
                Ok(DriveEnd::Ended)
            }
            Err(error) => {
                if error != DriveError::NoGame {
                    rebuild(&mut ctx, vm, &before, on_exit);
                }
                Err(error)
            }
        }
    }

    /// Whether the debugger holds the running game.
    pub fn debug_held(&self) -> bool {
        held(&self.state)
    }

    /// Ends the hold: the player's story input works again.
    pub fn debug_release(&mut self) {
        self.state.debug.held = None;
        self.state.debug.pause = false;
    }

    /// Holds the game at its current step, or at the next step it enters when it is not at
    /// one.
    pub fn debug_pause(&mut self) {
        let at = self.state.game.as_ref().and_then(|g| match &g.current {
            Current::Say(say) => Some((g.generation, say.step)),
            Current::Choice(choice) => Some((g.generation, choice.step)),
            Current::None => None,
        });
        match at {
            Some((generation, step)) => {
                self.state.debug.held = Some(generation);
                self.state.debug.stop = Some(LiveStop {
                    step,
                    reason: StopReason::Pause,
                });
            }
            None => self.state.debug.pause = true,
        }
    }

    /// Hold the live game when it enters one of `steps`.
    pub fn debug_set_breakpoints(&mut self, steps: impl IntoIterator<Item = StepId>) {
        self.state.debug.breakpoints = steps.into_iter().collect();
    }

    /// The stop the live game made by itself since the last call.
    pub fn debug_take_stop(&mut self) -> Option<LiveStop> {
        self.state.debug.stop.take()
    }

    /// Installs (or removes) the VM's instruction hook; returns the previous one.
    pub fn set_instr_hook(
        &mut self,
        hook: Option<Box<dyn InstrHook>>,
    ) -> Option<Box<dyn InstrHook>> {
        self.vm.set_hook(hook)
    }

    fn ensure_running(&self) -> Result<(), DriveError> {
        if self.running() {
            Ok(())
        } else {
            Err(DriveError::Stopped)
        }
    }

    fn parts<'a>(
        &'a mut self,
        platform: &'a mut dyn Platform,
        now_ms: u64,
    ) -> (HostCtx<'a>, &'a mut Vm) {
        let ctx = HostCtx {
            state: &mut self.state,
            platform,
            now_ms,
        };
        (ctx, &mut self.vm)
    }
}

/// The running game's exit handler when it runs the same entry scene as `trace`.
fn same_entry_exit(ctx: &HostCtx, trace: &Trace) -> Option<Value> {
    ctx.state
        .game
        .as_ref()
        .filter(|g| g.trace.entry == trace.entry)
        .and_then(|g| g.on_exit.clone())
}

/// Installs a driven game the way a load does, and holds it.
fn install(ctx: &mut HostCtx, vm: &mut Vm, frame: Box<GameFrame>) {
    let generation = frame.generation;
    ctx.state.pending_nav.push(NavOp::Restore(frame));
    if let Err(trap) = system::modes::settle(ctx, vm) {
        system::crash::crash(ctx, trap);
        return;
    }
    ctx.state.debug.held = Some(generation);
    ctx.state.debug.stop = None;
}

/// Replays `trace` and installs it after a live drive consumed the game (the restore drops
/// what is left of it).
fn rebuild(ctx: &mut HostCtx, vm: &mut Vm, trace: &Trace, on_exit: Option<Value>) {
    match replay::drive_trace(ctx, vm, trace, on_exit, &mut StopPilot) {
        Ok((frame, _)) => install(ctx, vm, frame),
        Err(error) => system::crash::crash(
            ctx,
            Trap::host(format!(
                "the debugger could not restore the story position: {error}"
            )),
        ),
    }
}
