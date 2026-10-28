// Moves by `amplitude`·(2/π)·Si(`rate`·t), the solution of dx/dt = amplitude·(2/π)·sin(rate·t)/t:
// an overshooting approach to `amplitude`.

struct Args {
    amplitude: vec2<f32>,
    rate: f32,
}

fn sinc(x: f32) -> f32 {
    if x == 0.0 {
        return 1.0;
    }
    return sin(x) / x;
}

// Si(x) = ∫₀ˣ sin(s)/s ds, composite Simpson with 64 intervals.
fn si(x: f32) -> f32 {
    let h = x / 64.0;
    var sum = sinc(0.0) + sinc(x);
    for (var i = 1; i < 64; i++) {
        let w = select(2.0, 4.0, (i % 2) == 1);
        sum += w * sinc(f32(i) * h);
    }
    return sum * h / 3.0;
}

fn effect(state: KgState, t: f32) -> KgState {
    var s = state;
    s.position += args.amplitude * (2.0 / KG_PI) * si(args.rate * t);
    return s;
}
