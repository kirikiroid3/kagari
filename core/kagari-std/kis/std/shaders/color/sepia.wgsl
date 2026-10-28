// CSS sepia(amount): 0 leaves the colour, 1 is fully sepia.

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

// W3C Filter Effects sepia(a), on straight sRGB.
fn sepia(c: vec3<f32>, amount: f32) -> vec3<f32> {
    let b = 1.0 - clamp(amount, 0.0, 1.0);
    return vec3<f32>(
        dot(vec3<f32>(0.393 + 0.607 * b, 0.769 - 0.769 * b, 0.189 - 0.189 * b), c),
        dot(vec3<f32>(0.349 - 0.349 * b, 0.686 + 0.314 * b, 0.168 - 0.168 * b), c),
        dot(vec3<f32>(0.272 - 0.272 * b, 0.534 - 0.534 * b, 0.131 + 0.869 * b), c),
    );
}

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let c = to_srgb(kg_source(uv));
    return from_srgb(vec4<f32>(sepia(c.rgb, args.amount), c.a));
}
