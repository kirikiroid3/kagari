#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::Viewport;
use crate::blur::gaussian_weights;
use crate::textures::thumbnail_problem;

#[test]
fn letterbox_fits_height() {
    let v = Viewport::letterbox((1280, 720), (1920, 1200));
    assert_eq!(
        v,
        Viewport {
            surface: (1920, 1200),
            x: 0,
            y: 60,
            width: 1920,
            height: 1080
        }
    );
}

#[test]
fn letterbox_fits_width() {
    let v = Viewport::letterbox((1280, 720), (1000, 1000));
    assert_eq!((v.x, v.y, v.width, v.height), (0, 218, 1000, 563));
}

#[test]
fn to_design_inverts_letterbox() {
    let v = Viewport::letterbox((1280, 720), (1920, 1200));
    assert_eq!(v.to_design((1280, 720), 960.0, 600.0), (640.0, 360.0));
    assert_eq!(v.to_design((1280, 720), 0.0, 30.0), (0.0, -20.0));
}

#[test]
fn gaussian_weights_are_normalised() {
    let w = gaussian_weights(2.0);
    assert_eq!(w.len(), 7);
    let total = w[0] + 2.0 * w[1..].iter().sum::<f32>();
    assert!((total - 1.0).abs() < 1e-6, "{total}");
}

#[test]
fn oversized_thumbnails_are_refused() {
    assert_eq!(thumbnail_problem(320, 180, 320 * 180 * 4, 8192), None);
    assert_eq!(
        thumbnail_problem(9000, 1, 9000 * 4, 8192).as_deref(),
        Some("is 9000×1, larger than the GPU limit 8192")
    );
    assert_eq!(
        thumbnail_problem(2, 2, 15, 8192).as_deref(),
        Some("has 15 bytes, expected 2×2×4")
    );
}
