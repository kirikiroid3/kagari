// Turns once clockwise per `period` seconds.

struct Args {
    period: f32,
}

fn effect(state: KgState, t: f32) -> KgState {
    var s = state;
    if args.period <= 0.0 {
        return s;
    }
    let phase = 2.0 * KG_PI * t / args.period;
    s.rotation += phase;
    return s;
}
