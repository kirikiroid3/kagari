// Gaussian blur with sigma `radius` px: 13×13 taps at k·σ/2 px, weights exp(−|k|²/8).

struct Args {
    radius: f32,
}

fn effect(uv: vec2<f32>) -> vec4<f32> {
    if args.radius <= 0.0 {
        return kg_source(uv);
    }
    let step = 0.5 * args.radius * kg_texel();
    var sum = vec4<f32>(0.0);
    var total = 0.0;
    for (var y = -6; y <= 6; y++) {
        for (var x = -6; x <= 6; x++) {
            let k = vec2<f32>(f32(x), f32(y));
            let w = exp(-dot(k, k) / 8.0);
            sum += w * kg_source(uv + k * step);
            total += w;
        }
    }
    return sum / total;
}
