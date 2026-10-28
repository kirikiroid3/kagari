
// The design target stretched over the viewport: a full-screen triangle, opaque output.
// `fs_present_srgb` writes linear light to an sRGB target (the hardware encodes);
// `fs_present_encode` encodes by hand for a non-sRGB target.

@group(1) @binding(0) var t0: texture_2d<f32>;
@group(1) @binding(1) var s0: sampler;

struct PresentOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_fullscreen(@builtin(vertex_index) index: u32) -> PresentOut {
    let x = f32((index << 1u) & 2u);
    let y = f32(index & 2u);
    var o: PresentOut;
    o.position = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    o.uv = vec2<f32>(x, y);
    return o;
}

@fragment
fn fs_present_srgb(i: PresentOut) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSample(t0, s0, i.uv).rgb, 1.0);
}

@fragment
fn fs_present_encode(i: PresentOut) -> @location(0) vec4<f32> {
    return vec4<f32>(linear_to_srgb(textureSample(t0, s0, i.uv).rgb), 1.0);
}
