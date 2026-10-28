//! Easings (easings.net, CSS `cubic-bezier` and jump-end `steps`), plus interpolation, motions
//! and transition progress (`interp.rs`). Easings are computed in `f64` on `x` clamped to
//! `[0, 1]`.

mod interp;
#[cfg(test)]
mod tests;

pub(crate) use interp::*;

use std::f64::consts::PI;

use crate::values::{Easing, NamedEasing, Tween};

const C1: f64 = 1.70158;
const C2: f64 = C1 * 1.525;
const C3: f64 = C1 + 1.0;
const C4: f64 = (2.0 * PI) / 3.0;
const C5: f64 = (2.0 * PI) / 4.5;
const N1: f64 = 7.5625;
const D1: f64 = 2.75;

/// `easing` at `x`.
pub(crate) fn ease(easing: &Easing, x: f32) -> f32 {
    let x = f64::from(x).clamp(0.0, 1.0);
    let y = match easing {
        Easing::Named(named) => named_ease(*named, x),
        Easing::Steps(n) => {
            if x >= 1.0 || *n == 0 {
                1.0
            } else {
                let n = f64::from(*n);
                (x * n).floor() / n
            }
        }
        Easing::CubicBezier([x1, y1, x2, y2]) => cubic_bezier(
            [
                decimal_f64(*x1),
                decimal_f64(*y1),
                decimal_f64(*x2),
                decimal_f64(*y2),
            ],
            x,
        ),
    };
    y as f32
}

/// A `Decimal` as `f64`: `mantissa / 10^scale`.
pub(crate) fn decimal_f64(d: kagari_vm::Decimal) -> f64 {
    let scale = i32::try_from(d.scale()).unwrap_or(i32::MAX);
    d.mantissa() as f64 / 10f64.powi(scale)
}

/// A `Decimal` as `f32`.
pub(crate) fn decimal_f32(d: kagari_vm::Decimal) -> f32 {
    decimal_f64(d) as f32
}

fn bounce_out(x: f64) -> f64 {
    if x < 1.0 / D1 {
        N1 * x * x
    } else if x < 2.0 / D1 {
        let x = x - 1.5 / D1;
        N1 * x * x + 0.75
    } else if x < 2.5 / D1 {
        let x = x - 2.25 / D1;
        N1 * x * x + 0.9375
    } else {
        let x = x - 2.625 / D1;
        N1 * x * x + 0.984375
    }
}

fn poly(k: i32, x: f64, mode: u8) -> f64 {
    match mode {
        0 => x.powi(k),
        1 => 1.0 - (1.0 - x).powi(k),
        _ => {
            if x < 0.5 {
                2f64.powi(k.saturating_sub(1)) * x.powi(k)
            } else {
                1.0 - (-2.0 * x + 2.0).powi(k) / 2.0
            }
        }
    }
}

fn named_ease(e: NamedEasing, x: f64) -> f64 {
    use NamedEasing as E;
    match e {
        E::Linear => x,
        E::InQuad => poly(2, x, 0),
        E::OutQuad => poly(2, x, 1),
        E::InOutQuad => poly(2, x, 2),
        E::InCubic => poly(3, x, 0),
        E::OutCubic => poly(3, x, 1),
        E::InOutCubic => poly(3, x, 2),
        E::InQuart => poly(4, x, 0),
        E::OutQuart => poly(4, x, 1),
        E::InOutQuart => poly(4, x, 2),
        E::InQuint => poly(5, x, 0),
        E::OutQuint => poly(5, x, 1),
        E::InOutQuint => poly(5, x, 2),
        E::InSine => 1.0 - (x * PI / 2.0).cos(),
        E::OutSine => (x * PI / 2.0).sin(),
        E::InOutSine => -((PI * x).cos() - 1.0) / 2.0,
        E::InExpo => {
            if x == 0.0 {
                0.0
            } else {
                2f64.powf(10.0 * x - 10.0)
            }
        }
        E::OutExpo => {
            if x == 1.0 {
                1.0
            } else {
                1.0 - 2f64.powf(-10.0 * x)
            }
        }
        E::InOutExpo => {
            if x == 0.0 || x == 1.0 {
                x
            } else if x < 0.5 {
                2f64.powf(20.0 * x - 10.0) / 2.0
            } else {
                (2.0 - 2f64.powf(-20.0 * x + 10.0)) / 2.0
            }
        }
        E::InCirc => 1.0 - (1.0 - x * x).max(0.0).sqrt(),
        E::OutCirc => (1.0 - (x - 1.0) * (x - 1.0)).max(0.0).sqrt(),
        E::InOutCirc => {
            if x < 0.5 {
                (1.0 - (1.0 - (2.0 * x).powi(2)).max(0.0).sqrt()) / 2.0
            } else {
                ((1.0 - (-2.0 * x + 2.0).powi(2)).max(0.0).sqrt() + 1.0) / 2.0
            }
        }
        E::InBack => C3 * x * x * x - C1 * x * x,
        E::OutBack => 1.0 + C3 * (x - 1.0).powi(3) + C1 * (x - 1.0).powi(2),
        E::InOutBack => {
            if x < 0.5 {
                (2.0 * x).powi(2) * ((C2 + 1.0) * 2.0 * x - C2) / 2.0
            } else {
                ((2.0 * x - 2.0).powi(2) * ((C2 + 1.0) * (2.0 * x - 2.0) + C2) + 2.0) / 2.0
            }
        }
        E::InElastic => {
            if x == 0.0 || x == 1.0 {
                x
            } else {
                -(2f64.powf(10.0 * x - 10.0)) * ((10.0 * x - 10.75) * C4).sin()
            }
        }
        E::OutElastic => {
            if x == 0.0 || x == 1.0 {
                x
            } else {
                2f64.powf(-10.0 * x) * ((10.0 * x - 0.75) * C4).sin() + 1.0
            }
        }
        E::InOutElastic => {
            if x == 0.0 || x == 1.0 {
                x
            } else if x < 0.5 {
                -(2f64.powf(20.0 * x - 10.0) * ((20.0 * x - 11.125) * C5).sin()) / 2.0
            } else {
                2f64.powf(-20.0 * x + 10.0) * ((20.0 * x - 11.125) * C5).sin() / 2.0 + 1.0
            }
        }
        E::InBounce => 1.0 - bounce_out(1.0 - x),
        E::OutBounce => bounce_out(x),
        E::InOutBounce => {
            if x < 0.5 {
                (1.0 - bounce_out(1.0 - 2.0 * x)) / 2.0
            } else {
                (1.0 + bounce_out(2.0 * x - 1.0)) / 2.0
            }
        }
    }
}

/// CSS `cubic-bezier(x1, y1, x2, y2)` at `x`.
fn cubic_bezier([x1, y1, x2, y2]: [f64; 4], x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let curve = |s: f64, p1: f64, p2: f64| {
        let r = 1.0 - s;
        3.0 * r * r * s * p1 + 3.0 * r * s * s * p2 + s * s * s
    };
    let slope = |s: f64, p1: f64, p2: f64| {
        let r = 1.0 - s;
        3.0 * r * r * p1 + 6.0 * r * s * (p2 - p1) + 3.0 * s * s * (1.0 - p2)
    };
    let mut s = x;
    let mut solved = false;
    for _ in 0..8 {
        let err = curve(s, x1, x2) - x;
        if err.abs() < 1e-6 {
            solved = true;
            break;
        }
        let d = slope(s, x1, x2);
        if d.abs() < 1e-6 {
            break;
        }
        s -= err / d;
    }
    if !solved || !(0.0..=1.0).contains(&s) {
        let (mut lo, mut hi) = (0.0f64, 1.0f64);
        s = x;
        for _ in 0..64 {
            let err = curve(s, x1, x2) - x;
            if err.abs() < 1e-6 {
                break;
            }
            if err > 0.0 {
                hi = s;
            } else {
                lo = s;
            }
            s = (lo + hi) / 2.0;
        }
    }
    curve(s, y1, y2)
}

/// A `Tween`'s progress `elapsed_ms` after the cursor: 0 during the delay, 1 once done.
pub(crate) fn tween_progress(t: &Tween, elapsed_ms: u64) -> f32 {
    let delay = u64::from(t.delay_ms);
    if elapsed_ms < delay {
        return 0.0;
    }
    let duration = u64::from(t.duration_ms);
    let run = elapsed_ms.saturating_sub(delay);
    if duration == 0 || run >= duration {
        return 1.0;
    }
    ease(&t.easing, (run as f64 / duration as f64) as f32)
}
