// A circular ramp: 0 at `center`, 1 at the farthest corner.

struct Args {
    center: vec2<f32>,
}

fn dist(v: vec2<f32>) -> f32 {
    return length(v);
}

// The largest distance from `c` to a corner of the box `size`, measured by `dist`.
fn farthest(c: vec2<f32>, size: vec2<f32>) -> f32 {
    let a = dist(c);
    let b = dist(vec2<f32>(size.x - c.x, c.y));
    let d = dist(vec2<f32>(c.x, size.y - c.y));
    let e = dist(size - c);
    return max(max(a, b), max(d, e));
}

fn effect(uv: vec2<f32>) -> f32 {
    let size = kg_size();
    let c = args.center * size;
    return clamp(dist(uv * size - c) / max(farthest(c, size), 1e-6), 0.0, 1.0);
}
