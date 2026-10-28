//! Interpolation of property values, colour conversion, relative motions and transition
//! progress.

use super::{decimal_f32, ease};
use crate::values::{Motion, Transition, TransitionKind};

pub(crate) fn lerp(a: f32, b: f32, p: f32) -> f32 {
    a + (b - a) * p
}

pub(crate) fn lerp4(a: [f32; 4], b: [f32; 4], p: f32) -> [f32; 4] {
    [
        lerp(a[0], b[0], p),
        lerp(a[1], b[1], p),
        lerp(a[2], b[2], p),
        lerp(a[3], b[3], p),
    ]
}

fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// An sRGB `Color` as premultiplied linear light.
pub(crate) fn color_premul_linear(c: [u8; 4]) -> [f32; 4] {
    let a = f32::from(c[3]) / 255.0;
    let channel = |v: u8| srgb_to_linear(f32::from(v) / 255.0) * a;
    [channel(c[0]), channel(c[1]), channel(c[2]), a]
}

/// A motion's `(offset, scale, rotation in degrees)` `elapsed_ms` after its start; identity
/// once every repeat ended.
pub(crate) fn motion_at(m: &Motion, elapsed_ms: u32) -> ([f32; 2], f32, f32) {
    const IDENTITY: ([f32; 2], f32, f32) = ([0.0, 0.0], 1.0, 0.0);
    let Some(last) = m.keys.last() else {
        return IDENTITY;
    };
    let period = last.at_ms;
    if period == 0 || u64::from(elapsed_ms) >= u64::from(period).saturating_mul(u64::from(m.repeat))
    {
        return IDENTITY;
    }
    let u = elapsed_ms.checked_rem(period).unwrap_or(0);
    let mut prev = (0u32, IDENTITY);
    for key in &m.keys {
        let value = (
            [key.offset[0] as f32, key.offset[1] as f32],
            decimal_f32(key.scale),
            decimal_f32(key.rotation),
        );
        if key.at_ms > u {
            let span = key.at_ms.saturating_sub(prev.0);
            let x = if span == 0 {
                1.0
            } else {
                u.saturating_sub(prev.0) as f32 / span as f32
            };
            let p = ease(&key.easing, x);
            let (from, to) = (prev.1, value);
            return (
                [lerp(from.0[0], to.0[0], p), lerp(from.0[1], to.0[1], p)],
                lerp(from.1, to.1, p),
                lerp(from.2, to.2, p),
            );
        }
        prev = (key.at_ms, value);
    }
    IDENTITY
}

/// A transition's progress `elapsed_ms` after its start, over `length_ms` (its duration, plus
/// the hold of `ThroughColor`).
pub(crate) fn transition_progress(tr: &Transition, elapsed_ms: u32, length_ms: u32) -> f32 {
    if length_ms == 0 || elapsed_ms >= length_ms {
        return 1.0;
    }
    let t = elapsed_ms as f32;
    match tr.kind {
        TransitionKind::ThroughColor { hold_ms, .. } => {
            let a = tr.duration_ms as f32 / 2.0;
            let hold = hold_ms as f32;
            let total = tr.duration_ms as f32 + hold;
            let h = if total == 0.0 { 0.0 } else { hold / total };
            let e = |x: f32| ease(&tr.easing, if a == 0.0 { 1.0 } else { x });
            if t < a {
                e(t / a) * (1.0 - h) / 2.0
            } else if t < a + hold {
                (1.0 - h) / 2.0 + ((t - a) / hold) * h
            } else {
                (1.0 + h) / 2.0 + e((t - a - hold) / a) * (1.0 - h) / 2.0
            }
        }
        _ => ease(&tr.easing, t / length_ms as f32),
    }
}
