// CSS saturate(amount): 0 is grey, 1 leaves the colour, above 1 oversaturates.

struct Args {
    amount: f32,
}

// Premultiplied linear → straight sRGB.
fn to_srgb(c: vec4<f32>) -> vec4<f32> {
    let s = kg_unpremultiply(c);
    return vec4<f32>(kg_linear_to_srgb(s.rgb), s.a);
}

// Straight sRGB → premultiplied linear, the colour clamped to 0..1.
fn from_srgb(c: vec4<f32>) -> vec4<f32> {
    let rgb = clamp(c.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    return kg_premultiply(vec4<f32>(kg_srgb_to_linear(rgb), c.a));
}

// W3C Filter Effects saturate(s), on straight sRGB.
fn saturation(c: vec3<f32>, amount: f32) -> vec3<f32> {
    let s = max(amount, 0.0);
    return vec3<f32>(
        dot(vec3<f32>(0.213 + 0.787 * s, 0.715 - 0.715 * s, 0.072 - 0.072 * s), c),
        dot(vec3<f32>(0.213 - 0.213 * s, 0.715 + 0.285 * s, 0.072 - 0.072 * s), c),
        dot(vec3<f32>(0.213 - 0.213 * s, 0.715 - 0.715 * s, 0.072 + 0.928 * s), c),
    );
}

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let c = to_srgb(kg_source(uv));
    return from_srgb(vec4<f32>(saturation(c.rgb, args.amount), c.a));
}
