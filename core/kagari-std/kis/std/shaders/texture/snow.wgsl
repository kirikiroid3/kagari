// Falling snow in three layers (scales 1, 0.7, 0.5) on a 48 px grid: a share `density`% of the
// cells hold a swaying flake of `radius` px falling at `speed` px/s.

struct Args {
    color: vec4<f32>,
    density: u32,
    speed: f32,
    radius: f32,
}

fn hash(v: vec2<f32>) -> f32 {
    return fract(sin(dot(v, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn layer(p: vec2<f32>, scale: f32, salt: f32) -> f32 {
    let cell_size = 48.0 * scale;
    let t = kg_time();
    let q = vec2<f32>(p.x, p.y - t * args.speed * scale);
    let base = floor(q / cell_size);
    let r = max(args.radius * scale, 0.001);
    var alpha = 0.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let cell = base + vec2<f32>(f32(x), f32(y));
            let h = hash(cell + salt);
            if h < f32(args.density) / 100.0 {
                let hx = hash(cell + salt + 0.37);
                let hy = hash(cell + salt + 0.71);
                let sway = sin(t + h * 6.28) * 8.0;
                let flake = (cell + vec2<f32>(hx, hy)) * cell_size + vec2<f32>(sway, 0.0);
                let d = length(q - flake);
                alpha = max(alpha, 1.0 - smoothstep(0.5 * r, r, d));
            }
        }
    }
    return alpha;
}

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let p = uv * kg_size();
    let a = max(layer(p, 1.0, 0.0), max(layer(p, 0.7, 13.0), layer(p, 0.5, 29.0)));
    return args.color * a;
}
