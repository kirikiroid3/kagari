// Mask role: `fn effect(uv: vec2<f32>) -> f32` in 0..1, uv 0..1 over the masked rect.

@group(2) @binding(0) var<storage, read> args: Args;

@fragment
fn kg_fs(i: KgVsOut) -> @location(0) vec4<f32> {
    let v = clamp(effect(i.local), 0.0, 1.0);
    return vec4<f32>(v, v, v, 1.0);
}
