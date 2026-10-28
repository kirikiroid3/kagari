// `count` linear ramps along `angle`.

struct Args {
    angle: f32,
    count: u32,
}

// 0 at the edge the direction `angle` starts from, 1 at the opposite edge.
fn linear_value(p: vec2<f32>, size: vec2<f32>, angle: f32) -> f32 {
    let d = vec2<f32>(cos(angle), sin(angle));
    let e = 0.5 * (abs(d.x) * size.x + abs(d.y) * size.y);
    return dot(p - 0.5 * size, d) / max(2.0 * e, 1e-6) + 0.5;
}

fn effect(uv: vec2<f32>) -> f32 {
    let size = kg_size();
    let n = f32(max(args.count, 1u));
    return clamp(fract(linear_value(uv * size, size, args.angle) * n), 0.0, 1.0);
}
