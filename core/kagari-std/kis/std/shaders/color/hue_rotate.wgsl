// CSS hue-rotate(angle); the angle is in radians.

struct Args {
    angle: f32,
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

// W3C Filter Effects hue-rotate(θ) = L + cos θ·C + sin θ·S, on straight sRGB.
fn hue_rotate(c: vec3<f32>, angle: f32) -> vec3<f32> {
    let k = cos(angle);
    let n = sin(angle);
    return vec3<f32>(
        dot(vec3<f32>(0.213 + 0.787 * k - 0.213 * n, 0.715 - 0.715 * k - 0.715 * n, 0.072 - 0.072 * k + 0.928 * n), c),
        dot(vec3<f32>(0.213 - 0.213 * k + 0.143 * n, 0.715 + 0.285 * k + 0.140 * n, 0.072 - 0.072 * k - 0.283 * n), c),
        dot(vec3<f32>(0.213 - 0.213 * k - 0.787 * n, 0.715 - 0.715 * k + 0.715 * n, 0.072 + 0.928 * k + 0.072 * n), c),
    );
}

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let c = to_srgb(kg_source(uv));
    return from_srgb(vec4<f32>(hue_rotate(c.rgb, args.angle), c.a));
}
