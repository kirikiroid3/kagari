// Shared by every shader module: the per-draw uniforms, the quad vertex stage and the sRGB
// curves. Each module is this file followed by its own (`concat!` in `pipelines.rs`).

struct Uniforms {
    target_size: vec2<f32>,
    pad0: vec2<f32>,
    a: vec4<f32>,
    b: vec4<f32>,
    c: vec4<f32>,
    weights: array<vec4<f32>, 25>,
}

@group(0) @binding(0) var<uniform> u: Uniforms;

struct VsIn {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) local: vec2<f32>,
}

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) local: vec2<f32>,
}

@vertex
fn vs_main(v: VsIn) -> VsOut {
    var o: VsOut;
    o.position = vec4<f32>(
        v.pos.x / u.target_size.x * 2.0 - 1.0,
        1.0 - v.pos.y / u.target_size.y * 2.0,
        0.0,
        1.0,
    );
    o.uv = v.uv;
    o.color = v.color;
    o.local = v.local;
    return o;
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let x = max(c, vec3<f32>(0.0));
    return select(pow((x + 0.055) / 1.055, vec3<f32>(2.4)), x / 12.92, x <= vec3<f32>(0.04045));
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let x = max(c, vec3<f32>(0.0));
    return select(1.055 * pow(x, vec3<f32>(1.0 / 2.4)) - 0.055, x * 12.92, x <= vec3<f32>(0.0031308));
}

// An image texel (premultiplied, sRGB-encoded) as premultiplied linear light.
fn decode_image(t: vec4<f32>) -> vec4<f32> {
    let a = t.a;
    let c = select(t.rgb / a, vec3<f32>(0.0), a == 0.0);
    return vec4<f32>(srgb_to_linear(c) * a, a);
}
