// Frosted glass (TyranoScript `[position_filter]`): where the entity has content, what is
// below it is blurred with sigma `radius` px and tinted, then the entity is drawn over it.

struct Args {
    radius: f32,
    tint: vec4<f32>,
}

fn blurred_backdrop(uv: vec2<f32>) -> vec4<f32> {
    if args.radius <= 0.0 {
        return kg_backdrop(uv);
    }
    let step = 0.5 * args.radius * kg_texel();
    var sum = vec4<f32>(0.0);
    var total = 0.0;
    for (var y = -6; y <= 6; y++) {
        for (var x = -6; x <= 6; x++) {
            let k = vec2<f32>(f32(x), f32(y));
            let w = exp(-dot(k, k) / 8.0);
            sum += w * kg_backdrop(uv + k * step);
            total += w;
        }
    }
    return sum / total;
}

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let s = kg_source(uv);
    let b = kg_backdrop(uv);
    let glass = args.tint + blurred_backdrop(uv) * (1.0 - args.tint.a);
    let under = mix(b, glass, select(0.0, 1.0, s.a > 0.0));
    return s + under * (1.0 - s.a);
}
