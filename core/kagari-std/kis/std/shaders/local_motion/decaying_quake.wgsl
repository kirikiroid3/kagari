// A shake that alternates every 1/60 s and fades out quadratically over `duration` seconds
// (TyranoScript `[quake2]`).

struct Args {
    horizontal: i32,
    vertical: i32,
    duration: f32,
}

fn effect(state: KgState, t: f32) -> KgState {
    var s = state;
    let n = floor(t * 60.0);
    let total = max(args.duration * 60.0, 1.0);
    let k = pow(1.0 - min(n / total, 1.0), 2.0);
    let sign = select(-1.0, 1.0, (i32(n) % 2) == 0);
    s.position += sign * k * vec2<f32>(f32(args.horizontal), f32(args.vertical));
    return s;
}
