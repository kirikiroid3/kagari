// Color role: `fn effect(uv: vec2<f32>) -> vec4<f32>`, uv = design-target uv 0..1.

@group(1) @binding(0) var kg_source_tex: texture_2d<f32>;
@group(1) @binding(1) var kg_sampler: sampler;
@group(2) @binding(0) var<storage, read> args: Args;

// The filtered content, premultiplied linear.
fn kg_source(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(kg_source_tex, kg_sampler, uv, 0.0);
}

@fragment
fn kg_fs(i: KgVsOut) -> @location(0) vec4<f32> {
    return effect(i.position.xy / kg_frame.target_size) * i.color;
}
