// Texture role: `fn effect(uv: vec2<f32>) -> vec4<f32>`, uv 0..1 over the paint rect.

@group(1) @binding(0) var kg_mask_tex: texture_2d<f32>;
@group(1) @binding(1) var kg_sampler: sampler;
@group(2) @binding(0) var<storage, read> args: Args;

@fragment
fn kg_fs(i: KgVsOut) -> @location(0) vec4<f32> {
    let m = textureSampleLevel(kg_mask_tex, kg_sampler, i.position.xy / kg_frame.target_size, 0.0).r;
    return effect(i.local) * m * i.color;
}
