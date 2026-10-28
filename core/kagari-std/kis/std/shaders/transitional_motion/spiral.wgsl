// Spirals into the end, turning `turns` times clockwise.

struct Args {
    turns: f32,
}

fn effect(start: KgState, end: KgState, progress: f32, t: f32) -> KgState {
    let p = clamp(progress, 0.0, 1.0);
    var s: KgState;
    let a = 2.0 * KG_PI * args.turns * p;
    let c = cos(a);
    let n = sin(a);
    let d = start.position - end.position;
    s.position = end.position + vec2<f32>(c * d.x - n * d.y, n * d.x + c * d.y) * (1.0 - p);
    s.scale = mix(start.scale, end.scale, p);
    s.rotation = mix(start.rotation, end.rotation, p);
    return s;
}
