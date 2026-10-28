// A Gaussian spot of standard deviation `sigma` px at `center`: 1 at the centre.

struct Args {
    center: vec2<f32>,
    sigma: f32,
}

fn effect(uv: vec2<f32>) -> f32 {
    if args.sigma <= 0.0 {
        return 0.0;
    }
    let size = kg_size();
    let d = uv * size - args.center * size;
    return clamp(exp(-dot(d, d) / (2.0 * args.sigma * args.sigma)), 0.0, 1.0);
}
