// Rises by `height` px and comes back once per `period` seconds.

struct Args {
    height: f32,
    period: f32,
}

fn effect(state: KgState, t: f32) -> KgState {
    var s = state;
    if args.period <= 0.0 {
        return s;
    }
    let phase = 2.0 * KG_PI * t / args.period;
    s.position.y -= args.height * (0.5 - 0.5 * cos(phase));
    return s;
}
