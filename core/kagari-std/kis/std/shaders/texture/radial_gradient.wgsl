// A radial gradient: `inner` at `center`, `outer` at the farthest corner.

struct Args {
    center: vec2<f32>,
    inner: vec4<f32>,
    outer: vec4<f32>,
}

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let size = kg_size();
    let c = args.center * size;
    let far = max(
        max(length(c), length(vec2<f32>(size.x - c.x, c.y))),
        max(length(vec2<f32>(c.x, size.y - c.y)), length(size - c)),
    );
    let v = clamp(length(uv * size - c) / max(far, 1e-6), 0.0, 1.0);
    return mix(args.inner, args.outer, v);
}
