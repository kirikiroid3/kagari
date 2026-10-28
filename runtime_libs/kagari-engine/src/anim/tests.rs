#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kagari_vm::Decimal;

use super::{ease, motion_at, transition_progress, tween_progress};
use crate::values::{Easing, Motion, MotionKey, NamedEasing, Transition, TransitionKind, Tween};

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

#[test]
fn easings() {
    assert_eq!(ease(&Easing::Named(NamedEasing::OutQuad), 0.5), 0.75);
    assert_eq!(ease(&Easing::Steps(4), 0.3), 0.25);
    assert_eq!(ease(&Easing::Steps(4), 1.0), 1.0);
    for named in (0..31).filter_map(NamedEasing::from_index) {
        let e = Easing::Named(named);
        assert!(close(ease(&e, 0.0), 0.0), "{named:?} at 0");
        assert!(close(ease(&e, 1.0), 1.0), "{named:?} at 1");
    }
    let bezier = Easing::CubicBezier([
        Decimal::new(25, 2),
        Decimal::new(1, 1),
        Decimal::new(25, 2),
        Decimal::ONE,
    ]);
    assert!(close(ease(&bezier, 0.5), 0.8024));
}

#[test]
fn tween_progress_counts_the_delay() {
    let t = Tween {
        duration_ms: 100,
        easing: Easing::LINEAR,
        delay_ms: 50,
    };
    assert_eq!(tween_progress(&t, 40), 0.0);
    assert!(close(tween_progress(&t, 100), 0.5));
    assert_eq!(tween_progress(&t, 150), 1.0);
}

fn key(at_ms: u32, dx: i32) -> MotionKey {
    MotionKey {
        at_ms,
        offset: [dx, 0],
        scale: Decimal::ONE,
        rotation: Decimal::ZERO,
        easing: Easing::LINEAR,
    }
}

#[test]
fn shake_offsets() {
    let shake = Motion {
        keys: vec![key(25, -12), key(75, 12), key(100, 0)],
        repeat: 4,
        delay_ms: 0,
    };
    for (elapsed, x) in [(25, -12.0), (50, 0.0), (75, 12.0), (125, -12.0), (400, 0.0)] {
        let (offset, scale, rotation) = motion_at(&shake, elapsed);
        assert!(close(offset[0], x), "at {elapsed}: {}", offset[0]);
        assert_eq!((scale, rotation), (1.0, 0.0));
    }
}

#[test]
fn through_color_progress() {
    let tr = Transition {
        kind: TransitionKind::ThroughColor {
            color: [0, 0, 0, 255],
            hold_ms: 200,
        },
        duration_ms: 1000,
        easing: Easing::LINEAR,
        delay_ms: 0,
    };
    assert!(close(transition_progress(&tr, 600, 1200), 0.5));
}
