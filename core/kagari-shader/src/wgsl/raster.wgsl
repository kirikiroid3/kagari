// Raster roles (color, mix, texture, mask): the renderer's frame uniform, the vertex stage.

// The first 64 bytes of the renderer's `Uniforms`.
struct KgFrame {
    target_size: vec2<f32>,
    xf_slot: u32,
    kg_pad: u32,
    a: vec4<f32>,
    b: vec4<f32>,
    c: vec4<f32>,
}

@group(0) @binding(0) var<uniform> kg_frame: KgFrame;
// Motion displacements: slot k is rows 2k = (a, c, e, 0) and 2k+1 = (b, d, f, 0).
@group(0) @binding(1) var<uniform> kg_transforms: array<vec4<f32>, 512>;

// Effect time in seconds.
fn kg_time() -> f32 {
    return kg_frame.a.x;
}

fn kg_opacity() -> f32 {
    return kg_frame.a.z;
}

// Size in px of the rect the effect covers.
fn kg_size() -> vec2<f32> {
    return kg_frame.b.zw;
}

fn kg_target_size() -> vec2<f32> {
    return kg_frame.target_size;
}

fn kg_texel() -> vec2<f32> {
    return 1.0 / kg_frame.target_size;
}

struct KgVsIn {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) local: vec2<f32>,
}

struct KgVsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) local: vec2<f32>,
}

fn kg_displace(p: vec2<f32>) -> vec2<f32> {
    let r0 = kg_transforms[2u * kg_frame.xf_slot];
    let r1 = kg_transforms[2u * kg_frame.xf_slot + 1u];
    return vec2<f32>(r0.x * p.x + r0.y * p.y + r0.z, r1.x * p.x + r1.y * p.y + r1.z);
}

@vertex
fn kg_vs(v: KgVsIn) -> KgVsOut {
    let p = kg_displace(v.pos);
    var o: KgVsOut;
    o.position = vec4<f32>(
        p.x / kg_frame.target_size.x * 2.0 - 1.0,
        1.0 - p.y / kg_frame.target_size.y * 2.0,
        0.0,
        1.0,
    );
    o.uv = v.uv;
    o.color = v.color;
    o.local = v.local;
    return o;
}
