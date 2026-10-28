// A parabolic arc that peaks `height` px above the straight line halfway.

struct Args {
    height: f32,
}

fn effect(start: KgState, end: KgState, progress: f32, t: f32) -> KgState {
    let p = clamp(progress, 0.0, 1.0);
    var s: KgState;
    s.position = mix(start.position, end.position, p) - vec2<f32>(0.0, args.height * 4.0 * p * (1.0 - p));
    s.scale = mix(start.scale, end.scale, p);
    s.rotation = mix(start.rotation, end.rotation, p);
    return s;
}
