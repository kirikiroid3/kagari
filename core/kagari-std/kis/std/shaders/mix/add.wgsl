// Additive compositing: the colours add, the alphas composite source-over.

alias Args = KgNoArgs;

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let s = kg_source(uv);
    let b = kg_backdrop(uv);
    return vec4<f32>(s.rgb + b.rgb, s.a + b.a * (1.0 - s.a));
}
