//! The story driver: resuming the fiber, entering steps, advancing, selecting, play modes.

use kagari_vm::{FnKind, StepId, Suspend, Text, Trap, Value, Vm};

use super::reveal::{RevealPlan, RevealRun, RevealState, Speeds};
use super::{
    BacklogEntry, ChoiceArmStep, ChoiceStep, Current, Entry, GameFrame, PlayMode, SayStep,
    StoryInput, text_plain,
};
use crate::HostCtx;
use crate::saves;
use crate::screen::runtime::mark_seen_images;
use crate::system::NavOp;
use crate::ui::SourceKey;
use crate::ui::imports::deref;

/// What a resume reached.
pub(crate) enum Resumed {
    Step,
    Returned(Value),
}

pub(crate) fn game_mut<'a>(ctx: &'a mut HostCtx) -> Result<&'a mut GameFrame, Trap> {
    ctx.state
        .game
        .as_mut()
        .ok_or_else(|| Trap::host("no running game to resume"))
}

/// Resumes the fiber with `input`; a reached step is entered.
pub(crate) fn resume(ctx: &mut HostCtx, vm: &mut Vm, input: Value) -> Result<Resumed, Trap> {
    let mut fiber = game_mut(ctx)?
        .fiber
        .take()
        .ok_or_else(|| Trap::host("no running game to resume"))?;
    let result = fiber.resume(vm, ctx, input);
    if let Some(game) = ctx.state.game.as_mut() {
        game.fiber = Some(fiber);
    }
    match result? {
        Suspend::Step { record, step } => {
            // A trap while entering the step names it, like a trap in story code after it.
            enter_step(ctx, vm, record, step).map_err(|mut trap| {
                trap.step.get_or_insert(step);
                trap
            })?;
            Ok(Resumed::Step)
        }
        Suspend::Returned(v) => Ok(Resumed::Returned(v)),
    }
}

/// Resumes a live game: a return finishes it.
pub(crate) fn resume_live(ctx: &mut HostCtx, vm: &mut Vm, input: Value) -> Result<(), Trap> {
    if let Resumed::Returned(v) = resume(ctx, vm, input)? {
        finish(ctx, vm, v)?;
    }
    Ok(())
}

fn option<'v>(ctx: &HostCtx, v: &'v Value) -> Result<Option<&'v Value>, Trap> {
    ctx.state.std.option(v)
}

fn opt_text(ctx: &HostCtx, v: &Value) -> Result<Option<Text>, Trap> {
    Ok(match option(ctx, v)? {
        Some(Value::Text(t)) => Some(t.clone()),
        Some(other) => {
            return Err(Trap::host(format!(
                "a step record holds a {} where a text belongs",
                other.tag()
            )));
        }
        None => None,
    })
}

fn invalidate(ctx: &mut HostCtx, keys: &[SourceKey]) {
    for key in keys {
        ctx.state.ui.invalidate(*key);
    }
}

/// Enters the step `record` reached at the STEP `step`.
fn enter_step(ctx: &mut HostCtx, vm: &mut Vm, record: Value, step: StepId) -> Result<(), Trap> {
    let replaying = ctx.state.system.replaying;
    let program = ctx.state.program.clone();
    let record = deref(vm, record)?;
    let Value::Adt(adt) = &record else {
        return Err(Trap::host(format!(
            "a step record must be `std::narrative::Say` or `std::narrative::Choice`, found `{}`",
            record.tag()
        )));
    };
    let is_say = match vm.types().path(adt.ty) {
        "std::narrative::Say" => true,
        "std::narrative::Choice" => false,
        other => {
            return Err(Trap::host(format!(
                "a step record must be `std::narrative::Say` or `std::narrative::Choice`, found `{other}`"
            )));
        }
    };
    let fields: Vec<Value> = adt
        .fields
        .iter()
        .map(|v| deref(vm, v.clone()))
        .collect::<Result<_, _>>()?;
    let field = |i: usize| -> Result<Value, Trap> {
        fields
            .get(i)
            .cloned()
            .ok_or_else(|| Trap::host("a malformed step record"))
    };
    let now = ctx.now_ms;
    let voice_sustain = ctx.state.system.flag(|s| s.voice_sustain, false);
    let animations = ctx.state.system.animations();
    let t = {
        let game = game_mut(ctx)?;
        game.step_serial = game.step_serial.saturating_add(1);
        let t = game.clock.story_ms(now);
        game.step_entered = t;
        game.skip_ready_at = None;
        t
    };
    if !replaying && !voice_sustain {
        ctx.state.audio.stop_voice();
    }
    // Render.
    let render = field(if is_say { 2 } else { 1 })?;
    let render = option(ctx, &render)?.cloned();
    let input = game_mut(ctx)?.screen.render_input();
    let out = match &render {
        Some(f) => vm.call_value(ctx, f, FnKind::Const, vec![input])?,
        None => input,
    };
    game_mut(ctx)?.render = render;
    {
        let state = &mut *ctx.state;
        let game = state
            .game
            .as_mut()
            .ok_or_else(|| Trap::host("no running game to resume"))?;
        game.screen
            .enter_step(out, t, animations, &mut state.assets, &mut *ctx.platform)?;
        if !replaying {
            mark_seen_images(game.screen.logical(), &mut state.system, &mut state.ui);
        }
    }
    let mut pushed = false;
    if is_say {
        let who = opt_text(ctx, &field(0)?)?;
        let voice = match option(ctx, &field(1)?)? {
            Some(Value::Asset(a)) => Some(*a),
            _ => None,
        };
        let text = opt_text(ctx, &field(3)?)?;
        let history = opt_text(ctx, &field(4)?)?;
        let read = ctx.state.system.stores.read.contains(step);
        let speeds = Speeds {
            text_speed_ms: ctx.state.system.duration_ms(|s| s.text_speed, 30),
        };
        let state = &mut *ctx.state;
        let game = state
            .game
            .as_mut()
            .ok_or_else(|| Trap::host("no running game to resume"))?;
        let at = game.trace_pos();
        game.backlog.push_back(BacklogEntry {
            entry: Entry::Line {
                who: who.clone(),
                text: history.or_else(|| text.clone()),
                voice,
                read,
            },
            at,
        });
        pushed = true;
        let text_start = game.screen.text_start_ms();
        game.reveal = RevealRun::new(
            RevealPlan::build(text.as_ref(), speeds),
            t.saturating_add(u64::from(text_start)),
        );
        game.current = Current::Say(SayStep {
            who,
            text,
            voice,
            step,
            read,
        });
        if !replaying && let Some(voice) = voice {
            let length = state.assets.audio_duration_ms(voice, &mut *ctx.platform)?;
            state
                .audio
                .play_voice(state.assets.path(voice), text_start, length, t);
        }
    } else {
        let id = match field(0)? {
            Value::Text(t) => text_plain(&t),
            Value::String(s) => s.to_string(),
            other => {
                return Err(Trap::host(format!(
                    "a choice id must be a text, found {}",
                    other.tag()
                )));
            }
        };
        let (Value::Array(items) | Value::Vec(items)) = field(2)? else {
            return Err(Trap::host("a malformed choice record"));
        };
        let mut arms = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            let item = deref(vm, item.clone())?;
            let Value::Adt(arm) = &item else {
                return Err(Trap::host("a malformed choice arm"));
            };
            let get = |i: usize| -> Result<Value, Trap> {
                deref(
                    vm,
                    arm.fields
                        .get(i)
                        .cloned()
                        .ok_or_else(|| Trap::host("a malformed choice arm"))?,
                )
            };
            let Value::Text(text) = get(0)? else {
                return Err(Trap::host("a malformed choice arm"));
            };
            let Value::Bool(shown) = get(1)? else {
                return Err(Trap::host("a malformed choice arm"));
            };
            let value = get(2)?;
            let arm_id = saves::trace::arm_id(vm, &value, index);
            arms.push(ChoiceArmStep {
                text,
                shown,
                value,
                arm_id,
            });
        }
        if !arms.iter().any(|a| a.shown) {
            return Err(Trap::host(format!("the choice `{id}` has no shown arm")));
        }
        let game = game_mut(ctx)?;
        let screen_step = game.screen.step();
        let disabled = screen_step.disabled_arms.clone();
        let align = screen_step.menu_align;
        let text_start = game.screen.text_start_ms();
        game.reveal = RevealRun::empty(t.saturating_add(u64::from(text_start)));
        game.current = Current::Choice(ChoiceStep {
            step,
            id,
            arms,
            disabled,
            align,
        });
    }
    if !replaying {
        log::info!(
            target: "kagari::story",
            "step {} {}",
            program.step_path(step),
            if is_say { "say" } else { "choice" }
        );
    }
    let backlog = ctx.state.system.config.as_ref().map_or(usize::MAX, |c| {
        usize::try_from(c.backlog).unwrap_or(usize::MAX)
    });
    {
        let state = &mut *ctx.state;
        if let Some(game) = state.game.as_mut() {
            while game.backlog.len() > backlog {
                game.backlog.pop_front();
            }
            if !replaying {
                let t = game.clock.story_ms(now);
                game.screen.tick(t, &game.reveal, &mut state.audio);
            }
        }
    }
    invalidate(
        ctx,
        &[
            SourceKey::NarrativeLine,
            SourceKey::NarrativeMenu,
            SourceKey::NarrativeHud,
        ],
    );
    if pushed {
        invalidate(ctx, &[SourceKey::NarrativeHistory]);
    }
    if !is_say && !replaying {
        saves::autosave(ctx, vm);
    }
    if !replaying {
        crate::debug::entered_live(ctx.state, step);
    }
    Ok(())
}

/// The next step after a `Say`. A return during replay is handed back, not finished.
pub(crate) fn next_step(ctx: &mut HostCtx, vm: &mut Vm) -> Result<Resumed, Trap> {
    let replaying = ctx.state.system.replaying;
    let step = match &game_mut(ctx)?.current {
        Current::Say(say) => Some(say.step),
        _ => None,
    };
    if let Some(step) = step
        && !replaying
    {
        ctx.state.system.stores.set_read(step);
    }
    let game = game_mut(ctx)?;
    game.trace.steps_after_last_choice = game.trace.steps_after_last_choice.saturating_add(1);
    let r = resume(ctx, vm, Value::unit())?;
    after(ctx, vm, r, replaying)
}

fn after(ctx: &mut HostCtx, vm: &mut Vm, r: Resumed, replaying: bool) -> Result<Resumed, Trap> {
    match r {
        Resumed::Returned(v) if !replaying => {
            finish(ctx, vm, v)?;
            Ok(Resumed::Step)
        }
        other => Ok(other),
    }
}

/// Selects arm `index` of the current choice.
pub(crate) fn select(ctx: &mut HostCtx, vm: &mut Vm, index: u32) -> Result<Resumed, Trap> {
    let replaying = ctx.state.system.replaying;
    let program = ctx.state.program.clone();
    let game = game_mut(ctx)?;
    let chosen = match &game.current {
        Current::Choice(choice) => choice
            .arms
            .get(index as usize)
            .filter(|a| a.shown)
            .map(|a| {
                (
                    a.text.clone(),
                    a.value.clone(),
                    saves::trace::choice_id(&program, choice.step.func, &choice.id, &a.arm_id),
                )
            }),
        _ => None,
    };
    let Some((text, value, choice_id)) = chosen else {
        log::warn!("ignored selection of arm {index}");
        return Ok(Resumed::Step);
    };
    let at = game.trace_pos();
    game.backlog.push_back(BacklogEntry {
        entry: Entry::Chosen(text),
        at,
    });
    game.trace.choices.push(choice_id.clone());
    game.trace.steps_after_last_choice = 0;
    if !replaying {
        log::info!(target: "kagari::story", "chose {choice_id}");
    }
    invalidate(ctx, &[SourceKey::NarrativeHistory]);
    let r = resume(ctx, vm, value)?;
    after(ctx, vm, r, replaying)
}

/// The entry scene returned.
pub(crate) fn finish(ctx: &mut HostCtx, vm: &mut Vm, value: Value) -> Result<(), Trap> {
    log::info!(target: "kagari::story", "story returned");
    let on_exit = {
        let game = game_mut(ctx)?;
        game.current = Current::None;
        game.on_exit.clone()
    };
    invalidate(
        ctx,
        &[
            SourceKey::NarrativeLine,
            SourceKey::NarrativeMenu,
            SourceKey::NarrativeHud,
        ],
    );
    if let Some(h) = on_exit {
        vm.call_value(ctx, &h, FnKind::Fn, vec![value])?;
    }
    if ctx.state.pending_nav.is_empty()
        && let Some(start) = ctx.state.system.config.as_ref().map(|c| c.start.clone())
    {
        ctx.state.pending_nav.push(NavOp::Goto(start));
    }
    Ok(())
}

/// Applies story input while the game is on top.
pub(crate) fn apply_input(ctx: &mut HostCtx, vm: &mut Vm, input: StoryInput) -> Result<(), Trap> {
    match input {
        StoryInput::Advance => advance(ctx, vm),
        StoryInput::Select(index) => select(ctx, vm, index).map(|_| ()),
    }
}

/// The Advance chain: complete the timeline and segment, release a click pause, or go on.
pub(crate) fn advance(ctx: &mut HostCtx, vm: &mut Vm) -> Result<(), Trap> {
    let now = ctx.now_ms;
    let state = &mut *ctx.state;
    let Some(game) = state.game.as_mut() else {
        return Ok(());
    };
    if !matches!(game.current, Current::Say(_)) {
        return Ok(());
    }
    let t = game.clock.story_ms(now);
    let reveal = game.reveal.state(t);
    if game.screen.timeline_running(t, &game.reveal) || reveal == RevealState::Revealing {
        game.screen.complete_all(t, &game.reveal, &mut state.audio);
        game.reveal.complete_segment(t);
        return Ok(());
    }
    if let RevealState::WaitingClick { .. } = reveal {
        game.reveal.release(t);
        return Ok(());
    }
    next_step(ctx, vm).map(|_| ())
}

/// Sets the play mode and invalidates it.
pub(crate) fn set_mode(ctx: &mut HostCtx, mode: PlayMode) {
    if let Some(game) = ctx.state.game.as_mut()
        && game.mode != mode
    {
        game.mode = mode;
        ctx.state.ui.invalidate(SourceKey::NarrativeMode);
    }
}

/// The play mode in effect: Skip while hold-skip is held.
pub(crate) fn effective_mode(ctx: &HostCtx) -> PlayMode {
    match &ctx.state.game {
        Some(_) if ctx.state.system.hold_skip => PlayMode::Skip,
        Some(game) => game.mode,
        None => PlayMode::Normal,
    }
}

/// Auto and Skip, once per `update` while the game is on top. Skip ends at a choice.
pub(crate) fn tick(ctx: &mut HostCtx, vm: &mut Vm) -> Result<(), Trap> {
    use crate::system::modes::ModeKind;
    if ctx.state.system.top() != Some(ModeKind::Game) || crate::debug::held(ctx.state) {
        return Ok(());
    }
    tick_mode(ctx, vm)?;
    let state = &mut *ctx.state;
    if let Some(game) = state.game.as_mut()
        && matches!(game.current, Current::Choice(_))
        && game.mode == PlayMode::Skip
    {
        game.mode = PlayMode::Normal;
        state.ui.invalidate(SourceKey::NarrativeMode);
    }
    Ok(())
}

/// Skip completes a step on the tick it reaches it, and goes on 50 ms later.
fn tick_mode(ctx: &mut HostCtx, vm: &mut Vm) -> Result<(), Trap> {
    let mode = effective_mode(ctx);
    let now = ctx.now_ms;
    let skip_unread = ctx.state.system.flag(|s| s.skip_unread, false);
    let auto_delay = u64::from(ctx.state.system.duration_ms(|s| s.auto_delay, 3000));
    let per_char = u64::from(ctx.state.system.duration_ms(|s| s.auto_delay_per_char, 0));
    let wait_for_voice = ctx.state.system.flag(|s| s.wait_for_voice, true);
    let state = &mut *ctx.state;
    let Some(game) = state.game.as_mut() else {
        return Ok(());
    };
    let t = game.clock.story_ms(now);
    let read = match &game.current {
        Current::None | Current::Choice(_) => return Ok(()),
        Current::Say(say) => say.read,
    };
    match mode {
        PlayMode::Skip => {
            if !read && !skip_unread {
                if game.mode == PlayMode::Skip {
                    game.mode = PlayMode::Normal;
                    state.ui.invalidate(SourceKey::NarrativeMode);
                }
                return Ok(());
            }
            match game.skip_ready_at {
                None => {
                    game.screen.complete_all(t, &game.reveal, &mut state.audio);
                    game.reveal.complete_all(t);
                    game.skip_ready_at = Some(t.saturating_add(50));
                    Ok(())
                }
                Some(ready) if t >= ready => {
                    next_step(ctx, vm)?;
                    tick_mode(ctx, vm)
                }
                Some(_) => Ok(()),
            }
        }
        PlayMode::Auto if !game.hide_window => match game.reveal.state(t) {
            RevealState::WaitingClick { since } => {
                if t >= since.saturating_add(auto_delay) {
                    game.reveal.release(t);
                }
                Ok(())
            }
            RevealState::Done { at } => {
                let due = at
                    .saturating_add(auto_delay)
                    .saturating_add(per_char.saturating_mul(u64::from(game.reveal.units())));
                let voice_done =
                    !wait_for_voice || state.audio.voice_end_ms().is_none_or(|end| t >= end);
                if t >= due && voice_done {
                    next_step(ctx, vm).map(|_| ())
                } else {
                    Ok(())
                }
            }
            RevealState::Revealing => Ok(()),
        },
        _ => Ok(()),
    }
}
