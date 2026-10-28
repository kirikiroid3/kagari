
// Textured quads: images (premultiplied sRGB), layers (premultiplied linear) and coverage masks.

@group(1) @binding(0) var t0: texture_2d<f32>;
@group(1) @binding(1) var s0: sampler;

@fragment
fn fs_image(i: VsOut) -> @location(0) vec4<f32> {
    return decode_image(textureSample(t0, s0, i.uv)) * i.color;
}

@fragment
fn fs_linear(i: VsOut) -> @location(0) vec4<f32> {
    return textureSample(t0, s0, i.uv) * i.color;
}

@fragment
fn fs_mask(i: VsOut) -> @location(0) vec4<f32> {
    return textureSample(t0, s0, i.uv).r * i.color;
}
