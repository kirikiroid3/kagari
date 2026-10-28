// The box with corners rounded by `radius` px and an edge softened over `feather` px.

struct Args {
    radius: f32,
    feather: f32,
}

fn effect(uv: vec2<f32>) -> f32 {
    let size = kg_size();
    let half = 0.5 * size;
    let r = clamp(args.radius, 0.0, min(half.x, half.y));
    let q = abs(uv * size - half) - (half - r);
    let d = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
    let f = max(args.feather, 0.001);
    return clamp(1.0 - smoothstep(-0.5 * f, 0.5 * f, d), 0.0, 1.0);
}
