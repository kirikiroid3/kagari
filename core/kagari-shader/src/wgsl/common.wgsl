// Kagari shader prelude: shared by every role. Prelude identifiers start with `kg_`/`Kg`.

const KG_PI: f32 = 3.14159265358979;

// The argument type of a shader whose Kagari argument type is `()`: `alias Args = KgNoArgs;`.
struct KgNoArgs {
    unused: u32,
}

fn kg_srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let x = max(c, vec3<f32>(0.0));
    return select(pow((x + 0.055) / 1.055, vec3<f32>(2.4)), x / 12.92, x <= vec3<f32>(0.04045));
}

fn kg_linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let x = max(c, vec3<f32>(0.0));
    return select(1.055 * pow(x, vec3<f32>(1.0 / 2.4)) - 0.055, x * 12.92, x <= vec3<f32>(0.0031308));
}

// Straight alpha from premultiplied; a fully transparent colour becomes 0.
fn kg_unpremultiply(c: vec4<f32>) -> vec4<f32> {
    if c.a == 0.0 {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(c.rgb / c.a, c.a);
}

fn kg_premultiply(c: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(c.rgb * c.a, c.a);
}
