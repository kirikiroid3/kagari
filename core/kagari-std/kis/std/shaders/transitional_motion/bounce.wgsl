// Overshoots the end by `overshoot` px, then settles (animate.css `bounceIn*`).

struct Args {
    overshoot: f32,
}

fn ease_out_cubic(x: f32) -> f32 {
    let y = 1.0 - x;
    return 1.0 - y * y * y;
}

// From `qa` at `pa` to `qb` at `pb`, easeOutCubic.
fn segment(p: f32, pa: f32, qa: f32, pb: f32, qb: f32) -> f32 {
    return qa + (qb - qa) * ease_out_cubic((p - pa) / (pb - pa));
}

fn effect(start: KgState, end: KgState, progress: f32, t: f32) -> KgState {
    let p = clamp(progress, 0.0, 1.0);
    var s: KgState;
    let u = end.position - start.position;
    let dist = length(u);
    var q = p;
    if dist > 0.0 {
        let k = args.overshoot / 25.0;
        let q1 = 1.0 + 25.0 * k / dist;
        let q2 = 1.0 - 10.0 * k / dist;
        let q3 = 1.0 + 5.0 * k / dist;
        if p < 0.6 {
            q = segment(p, 0.0, 0.0, 0.6, q1);
        } else if p < 0.75 {
            q = segment(p, 0.6, q1, 0.75, q2);
        } else if p < 0.9 {
            q = segment(p, 0.75, q2, 0.9, q3);
        } else {
            q = segment(p, 0.9, q3, 1.0, 1.0);
        }
    }
    s.position = start.position + u * q;
    s.scale = mix(start.scale, end.scale, p);
    s.rotation = mix(start.rotation, end.rotation, p);
    return s;
}
