// Grows to `scale` times its size and back once per `period` seconds.

struct Args {
    scale: f32,
    period: f32,
}

fn effect(state: KgState, t: f32) -> KgState {
    var s = state;
    if args.period <= 0.0 {
        return s;
    }
    let phase = 2.0 * KG_PI * t / args.period;
    s.scale *= 1.0 + (args.scale - 1.0) * (0.5 - 0.5 * cos(phase));
    return s;
}
