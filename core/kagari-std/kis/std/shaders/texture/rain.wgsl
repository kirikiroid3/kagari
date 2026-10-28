// Falling rain streaks: 8 px columns, a share `density`% of them holding a drop that falls
// `speed` px/s along `angle` (0 = straight down), `streak` px long.

struct Args {
    color: vec4<f32>,
    density: u32,
    speed: f32,
    angle: f32,
    streak: f32,
}

fn hash(x: f32) -> f32 {
    return fract(sin(x * 127.1 + 311.7) * 43758.5453);
}

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let size = kg_size();
    let c = cos(-args.angle);
    let n = sin(-args.angle);
    let d = uv * size - 0.5 * size;
    let q = vec2<f32>(c * d.x - n * d.y, n * d.x + c * d.y) + 0.5 * size;
    let column = floor(q.x / 8.0);
    let h = hash(column);
    if h >= f32(args.density) / 100.0 {
        return vec4<f32>(0.0);
    }
    let h2 = hash(column + 17.31);
    let height = max(size.y, 1.0);
    let head = fract(h2 + kg_time() * args.speed / height) * height;
    let len = max(args.streak, 1.0);
    let behind = head - q.y;
    if behind < 0.0 || behind > len {
        return vec4<f32>(0.0);
    }
    let along = 1.0 - smoothstep(0.0, len, behind);
    let across = clamp(1.0 - abs(q.x - column * 8.0 - 4.0) / 1.5, 0.0, 1.0);
    return args.color * (along * across);
}
