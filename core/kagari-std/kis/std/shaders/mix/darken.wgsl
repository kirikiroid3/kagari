// The `darken` blend mode of W3C Compositing Level 1, composited source-over.

alias Args = KgNoArgs;

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let s = kg_source(uv);
    let b = kg_backdrop(uv);
    let cs = kg_straight(s);
    let cb = kg_straight(b);
    return kg_composite(s, b, min(cb, cs));
}
