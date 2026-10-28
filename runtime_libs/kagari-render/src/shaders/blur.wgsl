
// One pass of a separable Gaussian blur. `u.a.xy` = step direction in texels, `u.a.z` = radius,
// `u.weights` = the weights `w_0..=w_radius`, four per vector. Texels outside the target are
// transparent.

@group(1) @binding(0) var t0: texture_2d<f32>;

fn tap(p: vec2<i32>, dims: vec2<i32>) -> vec4<f32> {
    if p.x < 0 || p.y < 0 || p.x >= dims.x || p.y >= dims.y {
        return vec4<f32>(0.0);
    }
    return textureLoad(t0, p, 0);
}

fn weight(k: i32) -> f32 {
    let v = u.weights[k / 4];
    return v[k % 4];
}

@fragment
fn fs_blur(i: VsOut) -> @location(0) vec4<f32> {
    let dims = vec2<i32>(textureDimensions(t0));
    let p = vec2<i32>(floor(i.position.xy));
    let dir = vec2<i32>(i32(u.a.x), i32(u.a.y));
    let r = i32(u.a.z);
    var acc = tap(p, dims) * weight(0);
    for (var k = 1; k <= r; k++) {
        acc += (tap(p + dir * k, dims) + tap(p - dir * k, dims)) * weight(k);
    }
    return acc;
}
