//! The Kagari standard library, written in Kagari and embedded as source text, and the WGSL
//! files of its shader library.

#![deny(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]

/// Path of the std root module, relative to the std directory.
pub const ROOT: &str = "mod.kis";

/// Declares `FILES` and `file` from one list, so that every listed path resolves.
macro_rules! std_files {
    ($($path:literal),* $(,)?) => {
        /// Every path `file` accepts: the `.kis` modules and the `.wgsl` shaders.
        pub const FILES: &[&str] = &[$($path),*];

        /// Contents of the std file at `path` (relative to the std directory, `/`-separated).
        pub fn file(path: &str) -> Option<&'static str> {
            Some(match path {
                $($path => include_str!(concat!("../kis/std/", $path)),)*
                _ => return None,
            })
        }
    };
}

std_files![
    "mod.kis",
    "prelude.kis",
    "time.kis",
    "screen.kis",
    "screen/anim.kis",
    "screen/audio.kis",
    "screen/shader.kis",
    "stage.kis",
    "narrative.kis",
    "narrative/text.kis",
    "ui.kis",
    "ui/style.kis",
    "system.kis",
    "system/nav.kis",
    "system/saves.kis",
    "shaders.kis",
    "shaders/color.kis",
    "shaders/mix.kis",
    "shaders/mask.kis",
    "shaders/texture.kis",
    "shaders/motion.kis",
    "shaders/color/adjust.wgsl",
    "shaders/color/blur.wgsl",
    "shaders/color/brightness.wgsl",
    "shaders/color/contrast.wgsl",
    "shaders/color/grayscale.wgsl",
    "shaders/color/hue_rotate.wgsl",
    "shaders/color/invert.wgsl",
    "shaders/color/saturate.wgsl",
    "shaders/color/sepia.wgsl",
    "shaders/local_motion/bob.wgsl",
    "shaders/local_motion/decaying_quake.wgsl",
    "shaders/local_motion/orbit.wgsl",
    "shaders/local_motion/pulse.wgsl",
    "shaders/local_motion/sine_integral.wgsl",
    "shaders/local_motion/spin.wgsl",
    "shaders/local_motion/sway.wgsl",
    "shaders/mask/checker.wgsl",
    "shaders/mask/clock.wgsl",
    "shaders/mask/diamond.wgsl",
    "shaders/mask/gaussian.wgsl",
    "shaders/mask/linear.wgsl",
    "shaders/mask/noise.wgsl",
    "shaders/mask/radial.wgsl",
    "shaders/mask/rounded_rect.wgsl",
    "shaders/mask/stripes.wgsl",
    "shaders/mix/add.wgsl",
    "shaders/mix/color.wgsl",
    "shaders/mix/color_burn.wgsl",
    "shaders/mix/color_dodge.wgsl",
    "shaders/mix/darken.wgsl",
    "shaders/mix/difference.wgsl",
    "shaders/mix/exclusion.wgsl",
    "shaders/mix/frosted_glass.wgsl",
    "shaders/mix/hard_light.wgsl",
    "shaders/mix/hue.wgsl",
    "shaders/mix/lighten.wgsl",
    "shaders/mix/luminosity.wgsl",
    "shaders/mix/multiply.wgsl",
    "shaders/mix/overlay.wgsl",
    "shaders/mix/saturation.wgsl",
    "shaders/mix/screen.wgsl",
    "shaders/mix/soft_light.wgsl",
    "shaders/texture/gaussian_spots.wgsl",
    "shaders/texture/linear_gradient.wgsl",
    "shaders/texture/radial_gradient.wgsl",
    "shaders/texture/rain.wgsl",
    "shaders/texture/snow.wgsl",
    "shaders/texture/solid.wgsl",
    "shaders/transitional_motion/arc.wgsl",
    "shaders/transitional_motion/bounce.wgsl",
    "shaders/transitional_motion/spiral.wgsl",
];

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::panic,
        clippy::expect_used,
        clippy::arithmetic_side_effects
    )]

    use super::{FILES, ROOT, file};

    #[test]
    fn every_listed_file_resolves() {
        assert!(FILES.contains(&ROOT));
        for path in FILES {
            assert!(file(path).is_some(), "`{path}` does not resolve");
        }
        assert!(file("missing.kis").is_none());
    }

    #[test]
    fn every_std_asset_is_a_listed_file() {
        const CALL: &str = "asset(\"";
        let mut found = 0;
        for path in FILES.iter().filter(|path| path.ends_with(".kis")) {
            let text = file(path).unwrap();
            for (at, _) in text.match_indices(CALL) {
                let rest = &text[at + CALL.len()..];
                let end = rest.find('"').expect("unterminated asset path");
                let asset = &rest[..end];
                assert!(
                    FILES.contains(&asset),
                    "`{path}` loads `{asset}`, which std does not ship"
                );
                found += 1;
            }
        }
        assert!(found > 0);
    }
}
