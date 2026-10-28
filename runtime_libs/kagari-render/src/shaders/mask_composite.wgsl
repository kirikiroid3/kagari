
// A layer times the red channel of a mask layer (`Node::Masked`), both sampled at the same uv.

@group(1) @binding(0) var t0: texture_2d<f32>;
@group(1) @binding(1) var t1: texture_2d<f32>;
@group(1) @binding(2) var s0: sampler;

@fragment
fn fs_mask_composite(i: VsOut) -> @location(0) vec4<f32> {
    return textureSample(t0, s0, i.uv) * textureSample(t1, s0, i.uv).r * i.color;
}
