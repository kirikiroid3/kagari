#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::rc::Rc;

use kagari_shader::ArgValue;
use kagari_vm::{AssetId, Decimal, HostObj, Value};

use super::model::{
    Animation, ChannelId, Cursor, EntityProp, LayerContent, LayerTarget, PartContent,
    PendingChange, PropTarget, PropValue, ScheduledKind, ScreenObj, Target,
};
use super::runtime::{NoReveal, ScreenRuntime};
use super::sample::VisualScreen;
use crate::HostCtx;
use crate::anim::decimal_f32;
use crate::audio::{AudioCommand, AudioModel};
use crate::frame::{MotionStage, ShaderEffect};
use crate::test_support::{FakePlatform, STD_TYPES, TestHost, engine};
use crate::values::{
    AudioFade, Easing, EffectSpec, Flip, LocalMotionBehavior, LoopSpec, Plane, PoseSpec, SoundSpec,
    Transition, TransitionKind, TransitionalMotionSpec, Tween, VAlign, Visibility,
};

fn tween(duration_ms: u32) -> Animation {
    Animation::Tween(Tween {
        duration_ms,
        easing: Easing::LINEAR,
        delay_ms: 0,
    })
}

fn dec(mantissa: i128, scale: u32) -> PropValue {
    PropValue::Decimal(Decimal::new(mantissa, scale))
}

fn opacity(id: u32) -> PendingChange {
    PendingChange::Prop(PropTarget::Entity(id, EntityProp::Opacity), dec(0, 0))
}

fn set_opacity(id: u32, value: PropValue) -> PendingChange {
    let PendingChange::Prop(target, _) = opacity(id) else {
        unreachable!()
    };
    PendingChange::Prop(target, value)
}

fn entity_opacity(v: &VisualScreen, id: u32) -> Option<f32> {
    v.entities.iter().find(|e| e.id == id).map(|e| e.opacity)
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

#[test]
fn spawn_defaults() {
    let mut screen = ScreenObj::empty();
    let e = screen.entity_mut(5).clone();
    assert_eq!(e.plane, Plane::Stage);
    assert_eq!((e.z, e.position), (0, [0, 0]));
    assert_eq!(e.anchor, [Decimal::ZERO, Decimal::ZERO]);
    assert_eq!(
        (e.scale, e.rotation, e.opacity),
        (Decimal::ONE, Decimal::ZERO, Decimal::ZERO)
    );
    assert_eq!((e.fill, e.fill_size), ([0, 0, 0, 0], None));
    assert!(e.parts.is_empty());
    assert_eq!((e.flip, e.gate), (Flip::None, None));
    assert!(e.filter.is_none() && e.mix.is_none() && e.mask.is_none());
    assert!(e.local_motion.is_none());
}

#[test]
fn empty_screen() {
    let screen = ScreenObj::empty();
    let background = &screen.logical.entities[&0];
    assert_eq!(background.plane, Plane::Background);
    assert_eq!(background.opacity, Decimal::ONE);
    assert_eq!(screen.logical.hud.text_window, Visibility::Shown);
    assert_eq!(screen.logical.hud.quick_menu, Visibility::Shown);
    assert_eq!(screen.logical.music.track, None);
    assert_eq!(screen.logical.music.volume, Decimal::ONE);
    assert!(
        screen
            .logical
            .ambience
            .iter()
            .all(|c| c.track.is_none() && c.volume == Decimal::ONE)
    );
    assert_eq!(screen.step.menu_align, VAlign::Middle);
}

#[test]
fn instant_commit() {
    let mut screen = ScreenObj::empty();
    screen.commit(set_opacity(1, dec(5, 1)), tween(0)).unwrap();
    assert_eq!(screen.logical.entities[&1].opacity, Decimal::new(5, 1));
    assert_eq!(screen.step.scheduled.len(), 1);
    assert_eq!(screen.step.scheduled[0].start, Cursor::ENTRY);
}

#[test]
fn opacity_out_of_range_is_rejected() {
    let err = ScreenObj::check_change(&set_opacity(1, dec(15, 1))).unwrap_err();
    assert_eq!(
        err.kind,
        kagari_vm::TrapKind::Host("opacity must be in 0.0..=1.0, found 1.5".into())
    );
}

fn runtime(base: &ScreenObj, screen: ScreenObj, animations: bool) -> ScreenRuntime {
    ScreenRuntime::from_step(base.logical.clone(), HostObj::new(screen), 0, animations)
}

#[test]
fn cut_rule() {
    let mut screen = ScreenObj::empty();
    screen.entity_mut(1);
    let base = screen.clone();
    screen
        .commit(set_opacity(1, dec(1, 0)), tween(1000))
        .unwrap();
    screen.wait(500).unwrap();
    screen
        .commit(set_opacity(1, dec(5, 1)), tween(1000))
        .unwrap();
    let rt = runtime(&base, screen, true);
    for (t, expected) in [(250, 0.25), (500, 1.0), (1000, 0.75), (1500, 0.5)] {
        let got = entity_opacity(&rt.sample(t), 1).unwrap();
        assert!(close(got, expected), "at {t}: {got}");
    }
}

#[test]
fn despawned_entities_draw_until_the_cursor_and_their_animations_end() {
    let mut screen = ScreenObj::empty();
    screen.entity_mut(1).opacity = Decimal::ONE;
    screen
        .entity_mut(1)
        .parts
        .insert(0, PartContent::Image(AssetId(0)));
    let base = screen.clone();
    // A fade-out longer than the cursor: drawn until the fade ends.
    let mut fading = screen.clone();
    fading
        .commit(set_opacity(1, dec(0, 0)), tween(300))
        .unwrap();
    fading.despawn(1);
    assert!(!fading.contains(1));
    let rt = runtime(&base, fading, true);
    assert!(close(entity_opacity(&rt.sample(150), 1).unwrap(), 0.5));
    assert_eq!(entity_opacity(&rt.sample(300), 1), None);
    // A fade-in, then the despawn after a wait: drawn until the cursor reaches the despawn,
    // and a barrier after it moves past it.
    screen.entity_mut(2);
    let base = screen.clone();
    screen
        .commit(set_opacity(2, dec(1, 0)), tween(500))
        .unwrap();
    screen.wait(3000).unwrap();
    screen.despawn(2);
    screen.barrier();
    screen.begin_text().unwrap();
    let mut rt = runtime(&base, screen, true);
    assert!(close(entity_opacity(&rt.sample(250), 2).unwrap(), 0.5));
    assert!(close(entity_opacity(&rt.sample(2999), 2).unwrap(), 1.0));
    assert_eq!(entity_opacity(&rt.sample(3000), 2), None);
    assert_eq!(rt.text_start_ms(), 3000);
    assert!(rt.timeline_running(2999, &NoReveal(0)));
    // A click completes the despawn with everything else.
    rt.complete_all(1000, &NoReveal(0), &mut AudioModel::new());
    assert_eq!(entity_opacity(&rt.sample(1000), 2), None);
}

#[test]
fn barrier_and_text_start() {
    let mut screen = ScreenObj::empty();
    let base = screen.clone();
    let fade = Transition {
        kind: TransitionKind::Crossfade,
        duration_ms: 800,
        easing: Easing::LINEAR,
        delay_ms: 0,
    };
    screen
        .commit(
            PendingChange::Layer(
                LayerTarget::Part(0, 0),
                Some(LayerContent::Image(AssetId(0))),
            ),
            Animation::Transition(fade),
        )
        .unwrap();
    screen.barrier();
    screen
        .commit(
            PendingChange::Prop(
                PropTarget::HudTextWindow,
                PropValue::Visibility(Visibility::Hidden),
            ),
            tween(200),
        )
        .unwrap();
    screen.begin_text().unwrap();
    let rt = runtime(&base, screen, true);
    assert_eq!(rt.text_start_ms(), 800);
    assert_eq!(rt.items[1].start, Some(800));
    assert!(close(rt.sample(900).hud.text_window, 0.5));
}

#[test]
fn text_start_skips_items_behind_a_mark_through_barriers() {
    let mut screen = ScreenObj::empty();
    screen.entity_mut(1);
    let base = screen.clone();
    screen.at_mark(1);
    screen
        .commit(set_opacity(1, dec(5, 1)), tween(500))
        .unwrap();
    screen.barrier();
    screen
        .commit(set_opacity(1, dec(1, 0)), tween(300))
        .unwrap();
    screen.barrier();
    screen.begin_text().unwrap();
    let rt = runtime(&base, screen, true);
    assert_eq!(rt.text_start_ms(), 0);
}

#[test]
fn animations_off_jump_to_the_target() {
    let mut screen = ScreenObj::empty();
    screen.entity_mut(1);
    let base = screen.clone();
    let delayed = Animation::Tween(Tween {
        duration_ms: 1000,
        easing: Easing::LINEAR,
        delay_ms: 500,
    });
    screen.commit(set_opacity(1, dec(1, 0)), delayed).unwrap();
    let rt = runtime(&base, screen, false);
    assert!(close(entity_opacity(&rt.sample(0), 1).unwrap(), 1.0));
}

fn sound(delay_ms: u32) -> SoundSpec {
    SoundSpec {
        audio: AssetId(1),
        path: Rc::from("s.ogg"),
        volume: Decimal::ONE,
        start_ms: 0,
        end_ms: None,
        delay_ms,
    }
}

#[test]
fn a_click_drops_one_shots_not_started() {
    let mut screen = ScreenObj::empty();
    let base = screen.clone();
    screen.play(sound(500), 1000).unwrap();
    let mut rt = runtime(&base, screen, true);
    let mut audio = AudioModel::new();
    audio.begin_game();
    rt.complete_all(100, &NoReveal(0), &mut audio);
    rt.tick(1000, &NoReveal(0), &mut audio);
    assert!(
        !audio
            .take_commands()
            .iter()
            .any(|c| matches!(c, AudioCommand::Play { .. }))
    );
}

#[test]
fn ticks_fire_sounds_at_their_start() {
    let mut screen = ScreenObj::empty();
    let base = screen.clone();
    screen.play(sound(500), 1000).unwrap();
    let mut rt = runtime(&base, screen, true);
    let mut audio = AudioModel::new();
    audio.begin_game();
    rt.tick(499, &NoReveal(0), &mut audio);
    assert!(audio.take_commands().is_empty());
    assert!(rt.timeline_running(499, &NoReveal(0)));
    rt.tick(500, &NoReveal(0), &mut audio);
    assert!(matches!(
        audio.take_commands().as_slice(),
        [AudioCommand::Play { path, .. }] if path == "s.ogg"
    ));
    assert!(!rt.timeline_running(1500, &NoReveal(0)));
}

#[test]
fn same_track_commit_schedules_nothing() {
    let track = LoopSpec {
        audio: AssetId(1),
        path: Rc::from("a.ogg"),
        loop_start_ms: 0,
        loop_end_ms: None,
    };
    let fade = Animation::Fade(AudioFade {
        fade_out_ms: 100,
        fade_in_ms: 100,
        overlap: true,
    });
    let mut screen = ScreenObj::empty();
    screen.logical.music.track = Some(track.clone());
    screen
        .commit(PendingChange::Channel(ChannelId::Music, Some(track)), fade)
        .unwrap();
    assert!(screen.step.scheduled.is_empty());
}

/// `shot()` of the fixture, run through the screen and audio imports.
#[test]
fn imports_build_a_screen() {
    let (mut vm, mut state) = engine(STD_TYPES);
    let program = vm.program().clone();
    let shot = program.find_function("crate::shot").unwrap();
    let mut platform = FakePlatform;
    let mut host = TestHost(HostCtx {
        state: &mut state,
        platform: &mut platform,
        now_ms: 0,
    });
    let out = vm.call(&mut host, shot, vec![]).unwrap();
    let Value::Host(obj) = out else {
        panic!("expected a screen");
    };
    let screen = obj.downcast::<ScreenObj>().unwrap();
    let hero = &screen.logical.entities[&1];
    assert_eq!(hero.opacity, Decimal::new(5, 1));
    assert_eq!(hero.z, 3);
    assert_eq!(hero.parts.len(), 1);
    assert_eq!(
        screen.logical.music.track.as_ref().map(|t| &*t.path),
        Some("a.ogg")
    );
    // opacity, image, music and the one-shot
    assert_eq!(screen.step.scheduled.len(), 4);
    assert_eq!(screen.step.scheduled[2].start.offset_ms, 50);
    assert_eq!(screen.step.scheduled[3].start.offset_ms, 60);
}

/// An effect of shader `shader` whose arguments are one `Decimal`.
fn effect(shader: u32, strength: Decimal, behavior: Option<LocalMotionBehavior>) -> EffectSpec {
    let arg_value = ArgValue::F32(decimal_f32(strength));
    EffectSpec {
        shader: AssetId(shader),
        args: Value::Decimal(strength),
        packed: Rc::from(kagari_shader::pack(&arg_value)),
        arg_value: Rc::new(arg_value),
        behavior,
    }
}

fn set_effect(target: PropTarget, e: EffectSpec) -> PendingChange {
    PendingChange::Prop(target, PropValue::Effect(Some(Rc::new(e))))
}

const FILTER_1: PropTarget = PropTarget::Entity(1, EntityProp::Filter);

/// The single `f32` argument of an effect.
fn arg(e: &ShaderEffect) -> f32 {
    f32::from_le_bytes(e.args[..4].try_into().unwrap())
}

fn entity_filter(v: &VisualScreen, id: u32) -> ShaderEffect {
    let entity = v.entities.iter().find(|e| e.id == id).unwrap();
    entity.filter.clone().unwrap()
}

#[test]
fn effect_arguments_interpolate_only_for_the_same_shader() {
    let mut screen = ScreenObj::empty();
    screen.entity_mut(1).filter = Some(Rc::new(effect(5, Decimal::ZERO, None)));
    let base = screen.clone();

    let mut same = screen.clone();
    same.commit(
        set_effect(FILTER_1, effect(5, Decimal::ONE, None)),
        tween(1000),
    )
    .unwrap();
    let rt = runtime(&base, same, true);
    let half = entity_filter(&rt.sample(500), 1);
    assert_eq!(half.shader, AssetId(5));
    assert!(close(arg(&half), 0.5), "{}", arg(&half));
    assert!(close(arg(&entity_filter(&rt.sample(1000), 1)), 1.0));

    screen
        .commit(
            set_effect(FILTER_1, effect(6, Decimal::ONE, None)),
            tween(1000),
        )
        .unwrap();
    let rt = runtime(&base, screen, true);
    let switched = entity_filter(&rt.sample(500), 1);
    assert_eq!(switched.shader, AssetId(6));
    assert!(close(arg(&switched), 1.0));
    // The clock of the new shader starts with its tween.
    assert!(close(switched.time, 0.5));
}

#[test]
fn local_motion_behaviours_map_time() {
    let stall = LocalMotionBehavior::Stall { after_ms: 1000 };
    assert_eq!(stall.map_ms(500), 500);
    assert_eq!(stall.map_ms(1000 + 600), 1000);
    let periodic = LocalMotionBehavior::Periodic {
        after_ms: 1000,
        period_ms: 400,
    };
    assert_eq!(periodic.map_ms(500), 500);
    assert_eq!(periodic.map_ms(1000 + 600), 1200);
    let held = LocalMotionBehavior::Periodic {
        after_ms: 1000,
        period_ms: 0,
    };
    assert_eq!(held.map_ms(5000), 1000);

    let mut screen = ScreenObj::empty();
    let base = screen.clone();
    let target = PropTarget::Entity(1, EntityProp::LocalMotion);
    screen
        .commit(
            set_effect(target, effect(7, Decimal::ZERO, Some(stall))),
            tween(0),
        )
        .unwrap();
    let rt = runtime(&base, screen, true);
    let v = rt.sample(1600);
    let entity = v.entities.iter().find(|e| e.id == 1).unwrap();
    let [MotionStage::Local { effect }] = entity.stages.as_slice() else {
        panic!("{:?}", entity.stages)
    };
    assert!(close(effect.time, 1.0));
}

fn motion(duration_ms: u32) -> TransitionalMotionSpec {
    TransitionalMotionSpec {
        effect: effect(9, Decimal::ZERO, None),
        duration_ms,
        easing: Easing::LINEAR,
        delay_ms: 0,
    }
}

#[test]
fn travel_sets_the_pose_and_schedules_three_tweens_and_the_travel() {
    let mut screen = ScreenObj::empty();
    screen.entity_mut(1).position = [10, 20];
    let base = screen.clone();
    let to = PoseSpec {
        position: [100, 50],
        scale: Decimal::new(2, 0),
        rotation: Decimal::new(90, 0),
    };
    screen.travel(1, to, motion(1000)).unwrap();
    let hero = &screen.logical.entities[&1];
    assert_eq!(
        (hero.position, hero.scale, hero.rotation),
        (to.position, to.scale, to.rotation)
    );
    let kinds: Vec<_> = screen
        .step
        .scheduled
        .iter()
        .map(|item| {
            (
                item.target,
                matches!(item.kind, ScheduledKind::Tween { .. }),
            )
        })
        .collect();
    assert_eq!(
        kinds,
        [
            (
                Some(Target::Prop(PropTarget::Entity(1, EntityProp::Position))),
                true
            ),
            (
                Some(Target::Prop(PropTarget::Entity(1, EntityProp::Scale))),
                true
            ),
            (
                Some(Target::Prop(PropTarget::Entity(1, EntityProp::Rotation))),
                true
            ),
            (Some(Target::Travel(1)), false),
        ]
    );
    let ScheduledKind::Travel { from, .. } = &screen.step.scheduled[3].kind else {
        panic!()
    };
    assert_eq!(from.position, [10, 20]);

    let rt = runtime(&base, screen.clone(), true);
    let v = rt.sample(250);
    let entity = v.entities.iter().find(|e| e.id == 1).unwrap();
    let [
        MotionStage::Transitional {
            effect,
            from,
            to,
            progress,
        },
    ] = entity.stages.as_slice()
    else {
        panic!("{:?}", entity.stages)
    };
    assert!(close(*progress, 0.25) && close(effect.time, 0.25));
    assert_eq!((from.position, to.position), ([10.0, 20.0], [100.0, 50.0]));
    assert!(close(to.rotation, std::f32::consts::FRAC_PI_2));
    let done = rt.sample(1000);
    assert!(done.entities.iter().all(|e| e.stages.is_empty()));
    // No travel without animations.
    let rt = runtime(&base, screen, false);
    assert!(rt.sample(0).entities.iter().all(|e| e.stages.is_empty()));
}

/// The asset id of `path` in the `STD_TYPES` program.
fn asset_id(state: &crate::EngineState, path: &str) -> AssetId {
    let program = state.program.clone();
    let index = program
        .module()
        .assets
        .iter()
        .position(|a| program.string(a.path) == path)
        .unwrap();
    AssetId(u32::try_from(index).unwrap())
}

fn next_screen(rt: &ScreenRuntime) -> ScreenObj {
    let Value::Host(obj) = rt.render_input() else {
        panic!()
    };
    obj.downcast::<ScreenObj>().unwrap().clone()
}

#[test]
fn effect_clock_carries_over_steps_until_the_shader_changes() {
    let (_vm, mut state) = engine(STD_TYPES);
    let glow = asset_id(&state, "glow.wgsl").0;
    let sepia = asset_id(&state, "sepia.wgsl").0;
    let filter = |shader: u32, strength: Decimal| {
        set_effect(PropTarget::ScreenFilter, effect(shader, strength, None))
    };
    let mut rt = ScreenRuntime::new(HostObj::new(ScreenObj::empty()));
    let mut enter = |rt: &mut ScreenRuntime, screen: ScreenObj, at: u64| {
        rt.enter_step(
            Value::Host(HostObj::new(screen)),
            at,
            true,
            &mut state.assets,
            &mut FakePlatform,
        )
        .unwrap();
    };

    let mut first = ScreenObj::empty();
    first.commit(filter(glow, Decimal::ZERO), tween(0)).unwrap();
    enter(&mut rt, first, 1000);
    assert!(close(rt.sample(1500).filter.unwrap().time, 0.5));

    // Same shader, new arguments: the clock keeps running.
    let mut second = next_screen(&rt);
    second
        .commit(filter(glow, Decimal::ONE), tween(500))
        .unwrap();
    enter(&mut rt, second, 3000);
    let carried = rt.sample(3500).filter.unwrap();
    assert!(close(carried.time, 2.5) && close(arg(&carried), 1.0));
    // No change at all: still running.
    let third = next_screen(&rt);
    enter(&mut rt, third, 4000);
    assert!(close(rt.sample(4000).filter.unwrap().time, 3.0));

    // Another shader: the clock restarts.
    let mut fourth = next_screen(&rt);
    fourth
        .commit(filter(sepia, Decimal::ONE), tween(0))
        .unwrap();
    enter(&mut rt, fourth, 5000);
    assert!(close(rt.sample(5500).filter.unwrap().time, 0.5));
}

/// `effects()` of the fixture, run through the screen imports.
#[test]
fn imports_build_effects() {
    let (mut vm, mut state) = engine(STD_TYPES);
    let program = vm.program().clone();
    let effects = program.find_function("crate::effects").unwrap();
    let mut platform = FakePlatform;
    let mut host = TestHost(HostCtx {
        state: &mut state,
        platform: &mut platform,
        now_ms: 0,
    });
    let Value::Host(obj) = vm.call(&mut host, effects, vec![]).unwrap() else {
        panic!("expected a screen");
    };
    let screen = obj.downcast::<ScreenObj>().unwrap();
    assert!(screen.logical.filter.is_some());
    let hero = &screen.logical.entities[&1];
    let filter = hero.filter.as_ref().unwrap();
    let white = ArgValue::Vec4([1.0; 4]);
    let spots = ArgValue::Array(vec![ArgValue::Vec2([1.0, 2.0]), ArgValue::Vec2([3.0, 4.0])]);
    assert_eq!(
        *filter.arg_value,
        ArgValue::Struct(vec![ArgValue::F32(1.0), white, spots, ArgValue::U32(1)])
    );
    // f32 @0, vec4 @16, array<vec2, 2> @32, u32 @48; rounded up to the struct's 16.
    assert_eq!(filter.packed.len(), 64);
    assert!(hero.mix.is_some() && hero.mask.is_some());
    assert_eq!(
        hero.local_motion.as_ref().unwrap().behavior,
        Some(LocalMotionBehavior::Stall { after_ms: 500 })
    );
    let Some(PartContent::Paint(paint)) = hero.parts.get(&0) else {
        panic!("{:?}", hero.parts)
    };
    assert_eq!(paint.size, [10, 20]);
    assert!(paint.mask.is_some());
    assert_eq!(hero.position, [100, 50]);
    assert_eq!(hero.rotation, Decimal::new(90, 0));
    assert!(
        screen
            .step
            .scheduled
            .iter()
            .any(|item| item.target == Some(Target::Travel(1)))
    );
    assert!(screen.step.scheduled.iter().any(|item| matches!(
        &item.kind,
        ScheduledKind::Transition { transition, .. }
            if matches!(transition.kind, TransitionKind::Masked { .. })
    )));
    // Two screens differing only in filter arguments are unequal.
    let mut other = screen.clone();
    let mut changed = (**filter).clone();
    changed.args = Value::Decimal(Decimal::ONE);
    other.entity_mut(1).filter = Some(Rc::new(changed));
    assert!(other.logical != screen.logical);
}
