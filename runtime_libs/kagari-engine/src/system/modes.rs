//! The mode stack and navigation (`docs/dev/src/runtime/engine.md`; for users,
//! `docs/reference/en/src/concepts/system.md#modes` and `#operations`).

use kagari_vm::{FuncId, HostObj, StoryFiber, StoryStatics, Trap, Value, Vm};

use crate::HostCtx;
use crate::saves;
use crate::screen::model::ScreenObj;
use crate::screen::runtime::ScreenRuntime;
use crate::story::narrative::NarrativeObj;
use crate::story::{GameFrame, Trace, driver};
use crate::ui::{self, ModeId};

/// The kind of a mode on the stack (`docs/reference/en/src/concepts/system.md#modes`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ModeKind {
    Root,
    Game,
    Overlay,
    GameOverlay,
    Dialog,
}

/// What `ui::mount` shows: the component of a mode and its argument.
#[derive(Clone, Debug)]
pub(crate) struct ModeFrame {
    pub kind: ModeKind,
    /// `fn() -> RootPage`, `fn() -> Overlay`, `fn() -> Dialog`, `fn(Narrative) -> GameOverlay` or
    /// the game view `fn(Narrative) -> Element`.
    pub component: Value,
    /// The `Narrative` handle passed to `Game` and `GameOverlay` components.
    pub narrative: Option<Value>,
    /// Whether the first focusable control takes focus when the mode opens (all but `Game`).
    pub autofocus: bool,
}

/// A mode on the stack.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Mode {
    pub kind: ModeKind,
    pub ui: ModeId,
}

/// A deferred navigation operation.
pub(crate) enum NavOp {
    Call(Value),
    Dialog(Value),
    CallGame {
        component: Value,
        generation: u64,
    },
    Back,
    Goto(Value),
    Start {
        entry: FuncId,
        args: Vec<Value>,
        on_exit: Value,
        seed: u64,
    },
    Restore(Box<GameFrame>),
    Quit,
}

/// Applies the deferred navigation and story input; runs after every outermost VM entry.
pub(crate) fn settle(ctx: &mut HostCtx, vm: &mut Vm) -> Result<(), Trap> {
    let result = settle_ops(ctx, vm);
    let ids: Vec<ModeId> = ctx.state.system.modes.iter().map(|m| m.ui).collect();
    if ids != ctx.state.system.logged_modes {
        let kinds: Vec<ModeKind> = ctx.state.system.modes.iter().map(|m| m.kind).collect();
        log::info!(target: "kagari::system", "modes: {kinds:?}");
        ctx.state.system.logged_modes = ids;
    }
    result
}

fn settle_ops(ctx: &mut HostCtx, vm: &mut Vm) -> Result<(), Trap> {
    loop {
        if ctx.state.system.crashed.is_some() || ctx.state.system.window.quit {
            return Ok(());
        }
        if !ctx.state.pending_nav.is_empty() {
            let op = ctx.state.pending_nav.remove(0);
            apply(ctx, vm, op)?;
            continue;
        }
        if ctx.state.system.top() == Some(ModeKind::Game)
            && let Some(input) = ctx.state.game.as_mut().and_then(|g| g.pending_input.take())
        {
            driver::apply_input(ctx, vm, input)?;
            continue;
        }
        return Ok(());
    }
}

/// Mounts a mode and pushes it, pausing a game it covers.
fn push(ctx: &mut HostCtx, vm: &mut Vm, kind: ModeKind, component: Value) -> Result<(), Trap> {
    if ctx.state.system.top() == Some(ModeKind::Game) {
        let now = ctx.now_ms;
        let state = &mut *ctx.state;
        if let Some(game) = state.game.as_mut() {
            game.clock.pause(now);
            let t = game.clock.story_ms(now);
            game.screen.complete_all(t, &game.reveal, &mut state.audio);
        }
    }
    let narrative = match kind {
        ModeKind::Game | ModeKind::GameOverlay => {
            let generation = ctx.state.game.as_ref().map_or(0, |g| g.generation);
            Some(Value::Host(HostObj::new(NarrativeObj { generation })))
        }
        _ => None,
    };
    let frame = ModeFrame {
        kind,
        component,
        narrative,
        autofocus: kind != ModeKind::Game,
    };
    let id = ui::mount(ctx, vm, &frame)?;
    ctx.state.system.modes.push(Mode { kind, ui: id });
    Ok(())
}

/// Unmounts every mode and drops the game.
fn clear(ctx: &mut HostCtx) {
    ctx.state.ui.unmount_all();
    ctx.state.system.modes.clear();
    if ctx.state.game.take().is_some() {
        ctx.state.audio.end_game();
    }
}

fn game_view(ctx: &HostCtx) -> Result<Value, Trap> {
    ctx.state
        .system
        .config
        .as_ref()
        .map(|c| c.game.clone())
        .ok_or_else(|| Trap::host("no `System` before `main` returned"))
}

/// Installs a game frame and mounts the game view.
fn install(ctx: &mut HostCtx, vm: &mut Vm, mut game: GameFrame) -> Result<(), Trap> {
    game.hud_shown = game.compute_hud_shown(ctx.now_ms);
    ctx.state.audio.begin_game();
    ctx.state.audio.sync_game(game.screen.logical());
    ctx.state.game = Some(game);
    let view = game_view(ctx)?;
    push(ctx, vm, ModeKind::Game, view)
}

fn apply(ctx: &mut HostCtx, vm: &mut Vm, op: NavOp) -> Result<(), Trap> {
    match op {
        NavOp::Call(f) => push(ctx, vm, ModeKind::Overlay, f),
        NavOp::Dialog(f) => push(ctx, vm, ModeKind::Dialog, f),
        NavOp::CallGame {
            component,
            generation,
        } => {
            let running = ctx
                .state
                .game
                .as_ref()
                .is_some_and(|g| g.generation == generation)
                && ctx.state.system.modes.first().map(|m| m.kind) == Some(ModeKind::Game);
            if running {
                push(ctx, vm, ModeKind::GameOverlay, component)
            } else {
                log::warn!("`Narrative::call` ignored: its game is not running");
                Ok(())
            }
        }
        NavOp::Back => {
            if ctx.state.system.modes.len() <= 1 {
                return Ok(());
            }
            if let Some(mode) = ctx.state.system.modes.pop() {
                ctx.state.ui.unmount(mode.ui);
            }
            if ctx.state.system.top() == Some(ModeKind::Game)
                && let Some(game) = ctx.state.game.as_mut()
            {
                game.clock.resume(ctx.now_ms);
            }
            Ok(())
        }
        NavOp::Goto(f) => {
            clear(ctx);
            push(ctx, vm, ModeKind::Root, f)
        }
        NavOp::Start {
            entry,
            args,
            on_exit,
            seed,
        } => {
            let program = ctx.state.program.clone();
            let fiber =
                StoryFiber::start(vm, entry, args.clone(), StoryStatics::fresh(&program), seed)?;
            clear(ctx);
            let generation = ctx.state.system.next_generation;
            ctx.state.system.next_generation = generation.saturating_add(1);
            let trace = Trace {
                entry: program.function_path(entry).to_owned(),
                args,
                seed,
                choices: Vec::new(),
                steps_after_last_choice: 0,
            };
            let screen = ScreenRuntime::new(HostObj::new(ScreenObj::empty()));
            let game = GameFrame::new(
                generation,
                entry,
                fiber,
                screen,
                trace,
                Some(on_exit),
                ctx.now_ms,
            );
            install(ctx, vm, game)?;
            driver::resume_live(ctx, vm, Value::unit())
        }
        NavOp::Restore(frame) => {
            clear(ctx);
            let mut frame = *frame;
            frame.clock = crate::story::StoryClock::new(ctx.now_ms);
            let voice = match &frame.current {
                crate::story::Current::Say(say) => say.voice,
                _ => None,
            };
            let text_start = frame.screen.text_start_ms();
            install(ctx, vm, frame)?;
            if let Some(voice) = voice {
                let state = &mut *ctx.state;
                let length = state.assets.audio_duration_ms(voice, &mut *ctx.platform)?;
                let t = state
                    .game
                    .as_ref()
                    .map_or(0, |g| g.clock.story_ms(ctx.now_ms));
                state
                    .audio
                    .play_voice(state.assets.path(voice), text_start, length, t);
            }
            Ok(())
        }
        NavOp::Quit => {
            quit_now(ctx, vm);
            Ok(())
        }
    }
}

/// Quits: autosaves a game at a step, writes the stores and sets `WindowState::quit`.
pub(crate) fn quit_now(ctx: &mut HostCtx, vm: &mut Vm) {
    if ctx.state.system.crashed.is_none()
        && ctx
            .state
            .game
            .as_ref()
            .is_some_and(|g| !matches!(g.current, crate::story::Current::None))
    {
        saves::autosave(ctx, vm);
    }
    crate::flush_stores(ctx, vm);
    ctx.state.system.window.quit = true;
}
