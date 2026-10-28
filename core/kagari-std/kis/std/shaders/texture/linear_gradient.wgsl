// A linear gradient along `angle` (0 = left to right, 90° = top to bottom).

struct Args {
    angle: f32,
    start: vec4<f32>,
    end: vec4<f32>,
}

// 0 at the edge the direction `angle` starts from, 1 at the opposite edge.
fn linear_value(p: vec2<f32>, size: vec2<f32>, angle: f32) -> f32 {
    let d = vec2<f32>(cos(angle), sin(angle));
    let e = 0.5 * (abs(d.x) * size.x + abs(d.y) * size.y);
    return dot(p - 0.5 * size, d) / max(2.0 * e, 1e-6) + 0.5;
}

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let size = kg_size();
    let v = clamp(linear_value(uv * size, size, args.angle), 0.0, 1.0);
    return mix(args.start, args.end, v);
}
