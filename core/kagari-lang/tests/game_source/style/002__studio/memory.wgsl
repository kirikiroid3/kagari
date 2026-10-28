// The flashback filter: desaturates the stage by `strength`.

struct Args {
    strength: f32,
}

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let c = kg_source(uv);
    let gray = dot(c.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    return vec4<f32>(mix(c.rgb, vec3<f32>(gray), clamp(args.strength, 0.0, 1.0)), c.a);
}
