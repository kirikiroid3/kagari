// Value noise with cells of `scale` px, varied by `seed`.

struct Args {
    scale: f32,
    seed: u32,
}

fn hash(cell: vec2<f32>) -> f32 {
    let q = cell + f32(args.seed) * 0.618;
    return fract(sin(dot(q, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn effect(uv: vec2<f32>) -> f32 {
    let g = uv * kg_size() / max(args.scale, 1.0);
    let i = floor(g);
    let f = fract(g);
    let w = f * f * (3.0 - 2.0 * f);
    let a = hash(i);
    let b = hash(i + vec2<f32>(1.0, 0.0));
    let c = hash(i + vec2<f32>(0.0, 1.0));
    let d = hash(i + vec2<f32>(1.0, 1.0));
    return clamp(mix(mix(a, b, w.x), mix(c, d, w.x), w.y), 0.0, 1.0);
}
