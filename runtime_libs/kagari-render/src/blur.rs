//! Gaussian weights of the separable shadow blur.

/// The largest σ, in px (a 64 px blur).
pub(crate) const MAX_SIGMA: f32 = 32.0;

/// The blur radius in taps for `sigma`: `ceil(3σ)`.
pub(crate) fn radius(sigma: f32) -> u32 {
    (3.0 * sigma).ceil().max(0.0) as u32
}

/// `w_i = exp(−i²/2σ²)` for `i in 0..=ceil(3σ)`, normalised so that `w_0 + 2·Σ_{i≥1} w_i = 1`.
/// `sigma` ≤ 0 gives `[1.0]`.
pub(crate) fn gaussian_weights(sigma: f32) -> Vec<f32> {
    if sigma <= 0.0 {
        return vec![1.0];
    }
    let r = radius(sigma);
    let weights: Vec<f32> = (0..=r)
        .map(|i| {
            let x = i as f32;
            (-(x * x) / (2.0 * sigma * sigma)).exp()
        })
        .collect();
    let total: f32 = weights
        .iter()
        .enumerate()
        .map(|(i, w)| if i == 0 { *w } else { 2.0 * w })
        .sum();
    weights.into_iter().map(|w| w / total).collect()
}

/// `weights` packed four per vector for the `weights` uniform array; extra weights are dropped.
pub(crate) fn pack(weights: &[f32]) -> [[f32; 4]; 25] {
    let mut out = [[0.0; 4]; 25];
    for (slot, chunk) in out.iter_mut().zip(weights.chunks(4)) {
        for (dst, w) in slot.iter_mut().zip(chunk) {
            *dst = *w;
        }
    }
    out
}
