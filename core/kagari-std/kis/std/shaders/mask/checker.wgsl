// A checkerboard of `columns` × `rows` cells, each sweeping left to right; the cells of one
// colour go first.

struct Args {
    columns: u32,
    rows: u32,
}

fn effect(uv: vec2<f32>) -> f32 {
    let q = uv * vec2<f32>(f32(max(args.columns, 1u)), f32(max(args.rows, 1u)));
    let parity = floor(q.x) + floor(q.y);
    let odd = parity - 2.0 * floor(parity / 2.0);
    return clamp((odd + fract(q.x)) / 2.0, 0.0, 1.0);
}
