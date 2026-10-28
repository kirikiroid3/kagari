// CSS brightness(amount): a linear multiplier on sRGB values.

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

// W3C Filter Effects brightness(b), on straight sRGB.
fn brightness(c: vec3<f32>, amount: f32) -> vec3<f32> {
    return max(amount, 0.0) * c;
}

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let c = to_srgb(kg_source(uv));
    return from_srgb(vec4<f32>(brightness(c.rgb, args.amount), c.a));
}
