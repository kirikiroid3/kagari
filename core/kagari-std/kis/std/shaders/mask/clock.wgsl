// A clock sweep around `center`, starting at `start` and turning clockwise.

struct Args {
    center: vec2<f32>,
    start: f32,
}

fn effect(uv: vec2<f32>) -> f32 {
    let size = kg_size();
    let d = uv * size - args.center * size;
    return clamp(fract((atan2(d.y, d.x) - args.start) / (2.0 * KG_PI)), 0.0, 1.0);
}
