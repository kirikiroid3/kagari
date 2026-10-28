// Shared by every shader module: the per-draw uniforms, the quad vertex stage and the sRGB
// curves. Each module is this file followed by its own (`concat!` in `pipelines.rs`).

// The first 64 bytes are `KgFrame` of the user shader prelude (`core/kagari-shader`).
struct Uniforms {
    target_size: vec2<f32>,
    // The motion displacement slot of the vertices (0 = identity).
    xf_slot: u32,
    pad0: u32,
    a: vec4<f32>,
    b: vec4<f32>,
    c: vec4<f32>,
    weights: array<vec4<f32>, 25>,
}

@group(0) @binding(0) var<uniform> u: Uniforms;
// Motion displacements: slot k is rows 2k = (a, c, e, 0) and 2k+1 = (b, d, f, 0).
@group(0) @binding(1) var<uniform> transforms: array<vec4<f32>, 512>;

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
    let r0 = transforms[2u * u.xf_slot];
    let r1 = transforms[2u * u.xf_slot + 1u];
    let p = vec2<f32>(r0.x * v.pos.x + r0.y * v.pos.y + r0.z, r1.x * v.pos.x + r1.y * v.pos.y + r1.z);
    var o: VsOut;
    o.position = vec4<f32>(
        p.x / u.target_size.x * 2.0 - 1.0,
        1.0 - p.y / u.target_size.y * 2.0,
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
