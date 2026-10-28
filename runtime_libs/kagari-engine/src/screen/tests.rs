#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::rc::Rc;

use kagari_vm::{AssetId, Decimal, HostObj, Value};

use super::model::{
    Animation, ChannelId, Cursor, EntityProp, LayerContent, LayerTarget, PendingChange, PropTarget,
    PropValue, ScreenObj,
};
use super::runtime::{NoReveal, ScreenRuntime};
use super::sample::VisualScreen;
use crate::HostCtx;
use crate::audio::{AudioCommand, AudioModel};
use crate::test_support::{FakePlatform, STD_TYPES, TestHost, engine};
use crate::values::{
    AudioFade, BlendMode, Easing, Flip, LoopSpec, Plane, SoundSpec, Transition, TransitionKind,
    Tween, VAlign, Visibility,
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
    assert_eq!(
        (e.flip, e.blend, e.gate),
        (Flip::None, BlendMode::Normal, None)
    );
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
fn despawned_entities_draw_until_their_animations_end() {
    let mut screen = ScreenObj::empty();
    screen.entity_mut(1).opacity = Decimal::ONE;
    screen.entity_mut(1).parts.insert(0, AssetId(0));
    let base = screen.clone();
    screen
        .commit(set_opacity(1, dec(0, 0)), tween(300))
        .unwrap();
    screen.despawn(1);
    assert!(!screen.contains(1));
    let rt = runtime(&base, screen, true);
    assert!(close(entity_opacity(&rt.sample(150), 1).unwrap(), 0.5));
    assert_eq!(entity_opacity(&rt.sample(300), 1), None);
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
