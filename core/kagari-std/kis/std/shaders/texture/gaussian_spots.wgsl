// The sum of up to 64 Gaussian spots; the sum is scaled back when its alpha exceeds 1.

struct GaussianSpot {
    position: vec2<f32>,
    color: vec4<f32>,
    radius: f32,
}

struct Args {
    count: u32,
    spots: array<GaussianSpot, 64>,
}

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let p = uv * kg_size();
    var sum = vec4<f32>(0.0);
    let n = min(args.count, 64u);
    for (var i = 0u; i < n; i++) {
        let spot = args.spots[i];
        if spot.radius > 0.0 {
            let d = p - spot.position;
            sum += spot.color * exp(-dot(d, d) / (2.0 * spot.radius * spot.radius));
        }
    }
    if sum.a > 1.0 {
        sum = sum / sum.a;
    }
    return sum;
}
