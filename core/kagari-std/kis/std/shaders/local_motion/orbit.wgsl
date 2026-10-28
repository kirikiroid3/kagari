// Circles clockwise on a circle of `radius` px once per `period` seconds, starting and
// ending at the pose.

struct Args {
    radius: f32,
    period: f32,
}

fn effect(state: KgState, t: f32) -> KgState {
    var s = state;
    if args.period <= 0.0 {
        return s;
    }
    let phase = 2.0 * KG_PI * t / args.period;
    s.position += args.radius * vec2<f32>(cos(phase) - 1.0, sin(phase));
    return s;
}
