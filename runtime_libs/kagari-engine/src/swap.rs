//! Scene hot swap (debugging only): a rebuilt program replaces the running one without losing
//! the story's position. The new program boots like a fresh start (`main`, the stores, the
//! fonts); the running story then moves over:
//!
//! - frames whose code did not change survive, mapped to the new program by function path;
//! - the outermost scene frame whose code changed is thrown off with every frame above it, and
//!   its call runs again with the new code, from the snapshot its caller's call recorded
//!   (story variables, the values behind `&mut` arguments, the PRNG, the trace position and
//!   the screen);
//! - when nothing on the stack changed, the current step is entered again;
//! - otherwise (the entry scene changed, story variables changed, a value cannot move, no
//!   snapshot) the story restarts from its entry with the same arguments and seed, and when
//!   even that fails the game is back at `System::start`.
//!
//! The mode stack is not kept: the game is installed like a load, held for the debugger.

use std::any::Any;
use std::rc::Rc;

use kagari_vm::{FnKind, FuncId, HostObj, Remap, RemapError, StoryFiber, Trap, Value};

use crate::debug::install;
use crate::saves::replay::{self, StopPilot};
use crate::screen::runtime::ScreenRuntime;
use crate::story::driver::{self, Resumed};
use crate::story::{BacklogEntry, CallMark, Entry, GameFrame, Trace, TracePos};
use crate::{BootError, Engine, Platform, flush_stores};

/// What the debugger found out about the rebuilt program.
pub struct SwapPlan<'a> {
    /// Whether the code a frame of this (old) function runs changed: its own body, or a
    /// non-scene function it runs inside its frame. Such a frame cannot survive.
    pub changed: &'a dyn Fn(FuncId) -> bool,
    /// Whether an old function and the new function at its path have the same code (closure
    /// values move only then).
    pub same_code: &'a dyn Fn(FuncId, FuncId) -> bool,
    /// A reason to restart the story from its entry that the caller found (story variables
    /// added, removed or retyped).
    pub restart: Option<String>,
}

/// Where the story went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SwapOutcome {
    /// No game was running at a step: the new program is at `System::start`.
    NoGame,
    /// No frame of the story changed: the current step was entered again.
    Kept,
    /// `scene`, the outermost changed scene, was called again by its caller.
    Recalled { scene: String },
    /// The story restarted from its entry scene with the same arguments and seed.
    Restarted { reason: String },
    /// The story could not restart either: the game is at `System::start`.
    Reset { reason: String },
}

/// The result of a swap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Swapped {
    pub outcome: SwapOutcome,
    /// The scene frames thrown off, outermost first.
    pub thrown: Vec<String>,
}

impl Engine {
    /// Replaces the running program with `bytes` (scene hot swap, see the module docs). On a
    /// boot failure the running program is kept. The debugger's hook, pause flag and call
    /// recording carry over; breakpoints do not (their step IDs name old functions).
    pub fn debug_swap(
        &mut self,
        bytes: &[u8],
        platform: &mut dyn Platform,
        now_ms: u64,
        plan: &SwapPlan<'_>,
    ) -> Result<Swapped, BootError> {
        if self.running() {
            let (mut ctx, vm) = self.parts(platform, now_ms);
            // The new program reads them back at boot.
            flush_stores(&mut ctx, vm);
        }
        platform.program_changed();
        let mut new = Engine::boot(bytes, platform, now_ms)?;
        new.vm.set_hook(self.vm.set_hook(None));
        new.state.debug.pause = self.state.debug.pause.clone();
        new.state.debug.record_calls = self.state.debug.record_calls;
        let game = if self.running() {
            self.state.game.take()
        } else {
            None
        };
        let swapped = match game {
            Some(game)
                if game.fiber.is_some()
                    && !matches!(game.current, crate::story::Current::None)
                    && new.running() =>
            {
                move_game(self, &mut new, game, plan, platform, now_ms)
            }
            _ => Swapped {
                outcome: SwapOutcome::NoGame,
                thrown: Vec::new(),
            },
        };
        let old_audio = std::mem::replace(&mut self.state.audio, crate::audio::AudioModel::new());
        new.state.audio.take_over(old_audio);
        new.now_ms = now_ms;
        *self = new;
        Ok(swapped)
    }
}

/// The story of `game` (of `old`) in `new`.
fn move_game(
    old: &Engine,
    new: &mut Engine,
    mut game: GameFrame,
    plan: &SwapPlan<'_>,
    platform: &mut dyn Platform,
    now_ms: u64,
) -> Swapped {
    let old_program = old.state.program.clone();
    let Some(mut fiber) = game.fiber.take() else {
        return Swapped {
            outcome: SwapOutcome::NoGame,
            thrown: Vec::new(),
        };
    };
    let is_scene = |f: FuncId| old_program.kind(f) == Some(FnKind::Scene);
    let frames: Vec<FuncId> = fiber.frames().iter().map(|f| f.func).collect();
    // The outermost frame whose code changed, widened to the scene frame that runs it.
    let cut = frames.iter().position(|f| (plan.changed)(*f)).map(|i| {
        (0..=i)
            .rev()
            .find(|&j| frames.get(j).is_some_and(|f| is_scene(*f)))
            .unwrap_or(0)
    });
    let thrown: Vec<String> = cut
        .map(|d| {
            frames
                .iter()
                .skip(d)
                .filter(|f| is_scene(**f))
                .map(|f| old_program.function_path(*f).to_owned())
                .collect()
        })
        .unwrap_or_default();
    let fall_back = |new: &mut Engine,
                     platform: &mut dyn Platform,
                     reason: String,
                     thrown: Vec<String>| Swapped {
        outcome: restart_story(old, new, &game, reason, platform, now_ms),
        thrown,
    };
    let path = |d: usize| {
        frames
            .get(d)
            .map_or("", |f| old_program.function_path(*f))
            .to_owned()
    };
    let refused = plan.restart.clone().or_else(|| match cut {
        Some(0) => Some(format!("the entry scene `{}` changed", path(0))),
        Some(d) if !fiber.has_call_snapshot(d) => Some(format!(
            "the call of `{}` has no snapshot (snapshots start when the debugger asks for \
             them)",
            path(d)
        )),
        None if fiber.call_mark().is_none() => Some("the current step has no snapshot".to_owned()),
        _ => None,
    });
    if let Some(reason) = refused {
        return fall_back(new, platform, reason, thrown);
    }
    let mark = match cut {
        Some(d) => match fiber.unwind_to_call(&old_program, d) {
            Ok(mark) => mark,
            Err(error) => return fall_back(new, platform, error, thrown),
        },
        None => fiber.call_mark().cloned(),
    };
    let Some(mark) = mark
        .as_deref()
        .and_then(|m| m.downcast_ref::<CallMark>())
        .cloned()
    else {
        return fall_back(new, platform, "the call has no mark".to_owned(), thrown);
    };
    let pos = match cut {
        Some(_) => mark.pos,
        None => game.trace_pos(),
    };
    let moved = moved(old, new, &game, &fiber, &mark, pos, plan);
    let (fiber, screen, entry, args, on_exit, backlog) = match moved {
        Ok(moved) => moved,
        Err(error) => return fall_back(new, platform, error.to_string(), thrown),
    };
    let generation = new.state.system.next_generation;
    new.state.system.next_generation = generation.saturating_add(1);
    let choices = game
        .trace
        .choices
        .iter()
        .take(pos.choices as usize)
        .cloned()
        .collect();
    let trace = Trace {
        entry: game.trace.entry.clone(),
        args,
        seed: game.trace.seed,
        choices,
        steps_after_last_choice: pos.steps,
    };
    let mut frame = GameFrame::new(
        generation,
        entry,
        fiber,
        ScreenRuntime::new(screen),
        trace,
        on_exit,
        now_ms,
    );
    frame.backlog = backlog;
    let (mut ctx, vm) = new.parts(platform, now_ms);
    let was_replaying = ctx.state.system.replaying;
    ctx.state.system.replaying = true;
    ctx.state.game = Some(frame);
    let result = vm.with_fresh_budget(|vm| match cut {
        Some(_) => driver::resume(&mut ctx, vm, Value::unit()),
        None => driver::reenter(&mut ctx, vm),
    });
    ctx.state.system.replaying = was_replaying;
    let frame = ctx.state.game.take();
    let program = ctx.state.program.clone();
    let failed = match (result, frame) {
        (Ok(Resumed::Step), Some(frame)) => {
            install(&mut ctx, vm, Box::new(frame));
            None
        }
        (Ok(_), _) => Some("the story returned right after the swap".to_owned()),
        (Err(trap), _) => Some(format!(
            "the new code panicked: {}",
            first_line(&trap, &program)
        )),
    };
    match failed {
        None => Swapped {
            outcome: match cut {
                Some(d) => SwapOutcome::Recalled { scene: path(d) },
                None => SwapOutcome::Kept,
            },
            thrown,
        },
        Some(reason) => fall_back(new, platform, reason, thrown),
    }
}

/// What moves: the fiber, the screen at the mark, the entry, its arguments, the exit handler
/// and the backlog before `pos`.
type Moved = (
    StoryFiber,
    HostObj,
    FuncId,
    Vec<Value>,
    Option<Value>,
    std::collections::VecDeque<BacklogEntry>,
);

fn moved(
    old: &Engine,
    new: &mut Engine,
    game: &GameFrame,
    fiber: &StoryFiber,
    mark: &CallMark,
    pos: TracePos,
    plan: &SwapPlan<'_>,
) -> Result<Moved, RemapError> {
    let old_program = old.state.program.clone();
    let new_program = new.state.program.clone();
    let mut r = Remap::new(
        &old_program,
        old.vm.types(),
        &new_program,
        new.vm.types_mut(),
        plan.same_code,
    );
    let fiber = fiber.transplant(&mut r, &mut |m, r| remap_mark(m, r))?;
    let Value::Host(screen) = r.value(&mark.screen)? else {
        return Err(RemapError::Host("screen".to_owned()));
    };
    let entry = r.func(game.entry)?;
    let args = game
        .trace
        .args
        .iter()
        .map(|a| r.value(a))
        .collect::<Result<_, _>>()?;
    // An exit handler that cannot move is dropped: the end of the story goes to the start.
    let on_exit = game.on_exit.as_ref().and_then(|h| r.value(h).ok());
    // Backlog lines that cannot move (a voice that is gone) are dropped.
    let backlog = game
        .backlog
        .iter()
        .filter(|b| b.at < pos)
        .filter_map(|b| {
            let entry = match &b.entry {
                Entry::Line {
                    who,
                    text,
                    voice,
                    read,
                } => Entry::Line {
                    who: who.as_ref().map(|t| r.text(t)).transpose().ok()?,
                    text: text.as_ref().map(|t| r.text(t)).transpose().ok()?,
                    voice: voice.map(|v| r.asset(v)).transpose().ok()?,
                    read: *read,
                },
                Entry::Chosen(text) => Entry::Chosen(r.text(text).ok()?),
            };
            Some(BacklogEntry { entry, at: b.at })
        })
        .collect();
    Ok((fiber, screen, entry, args, on_exit, backlog))
}

fn remap_mark(mark: &Rc<dyn Any>, r: &mut Remap<'_>) -> Result<Rc<dyn Any>, RemapError> {
    let mark = mark
        .downcast_ref::<CallMark>()
        .ok_or_else(|| RemapError::Host("call mark".to_owned()))?;
    Ok(Rc::new(CallMark {
        pos: mark.pos,
        screen: r.value(&mark.screen)?,
    }))
}

/// Restarts the story of `game` from its entry in `new`, with the same arguments and seed;
/// when that fails too, the game stays at `System::start`.
fn restart_story(
    old: &Engine,
    new: &mut Engine,
    game: &GameFrame,
    reason: String,
    platform: &mut dyn Platform,
    now_ms: u64,
) -> SwapOutcome {
    let started = {
        let old_program = old.state.program.clone();
        let new_program = new.state.program.clone();
        let same = |_: FuncId, _: FuncId| false;
        let mut r = Remap::new(
            &old_program,
            old.vm.types(),
            &new_program,
            new.vm.types_mut(),
            &same,
        );
        game.trace
            .args
            .iter()
            .map(|a| r.value(a))
            .collect::<Result<Vec<_>, _>>()
            .map(|args| (args, game.on_exit.as_ref().and_then(|h| r.value(h).ok())))
    };
    let (args, on_exit) = match started {
        Ok(started) => started,
        Err(error) => {
            return SwapOutcome::Reset {
                reason: format!("{reason}; the entry's arguments cannot move: {error}"),
            };
        }
    };
    let trace = Trace {
        entry: game.trace.entry.clone(),
        args,
        seed: game.trace.seed,
        choices: Vec::new(),
        steps_after_last_choice: 0,
    };
    let (mut ctx, vm) = new.parts(platform, now_ms);
    match replay::drive_trace(&mut ctx, vm, &trace, on_exit, &mut StopPilot) {
        Ok((frame, _)) => {
            install(&mut ctx, vm, frame);
            SwapOutcome::Restarted { reason }
        }
        Err(error) => SwapOutcome::Reset {
            reason: format!("{reason}; restarting from the entry failed too: {error}"),
        },
    }
}

fn first_line(trap: &Trap, program: &kagari_vm::Program) -> String {
    trap.render(program)
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned()
}
