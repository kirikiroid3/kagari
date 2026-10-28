//! Kagari shader roles, the WGSL preludes every user shader is composed with, argument packing
//! and (feature `validate`) naga validation.
#![deny(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]

mod args;
#[cfg(feature = "validate")]
mod validate;

#[cfg(test)]
mod tests;

pub use args::{ArgValue, lerp, pack};
#[cfg(feature = "validate")]
pub use validate::{ShaderError, validate};

/// What a shader file does; decides its prelude, entry point and pipeline layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ShaderRole {
    Color,
    Mix,
    Texture,
    Mask,
    LocalMotion,
    TransitionalMotion,
}

impl ShaderRole {
    pub const ALL: [ShaderRole; 6] = [
        ShaderRole::Color,
        ShaderRole::Mix,
        ShaderRole::Texture,
        ShaderRole::Mask,
        ShaderRole::LocalMotion,
        ShaderRole::TransitionalMotion,
    ];

    /// The std type (in `std::screen::shader`) of an asset of this role.
    pub fn type_name(self) -> &'static str {
        match self {
            ShaderRole::Color => "ColorShader",
            ShaderRole::Mix => "MixShader",
            ShaderRole::Texture => "TextureShader",
            ShaderRole::Mask => "MaskShader",
            ShaderRole::LocalMotion => "LocalMotionShader",
            ShaderRole::TransitionalMotion => "TransitionalMotionShader",
        }
    }

    /// Human-readable name for messages.
    pub fn name(self) -> &'static str {
        match self {
            ShaderRole::Color => "color",
            ShaderRole::Mix => "mix",
            ShaderRole::Texture => "texture",
            ShaderRole::Mask => "mask",
            ShaderRole::LocalMotion => "local motion",
            ShaderRole::TransitionalMotion => "transitional motion",
        }
    }

    /// Motion roles run as compute shaders; the rest are raster (vertex + fragment).
    pub fn is_compute(self) -> bool {
        matches!(
            self,
            ShaderRole::LocalMotion | ShaderRole::TransitionalMotion
        )
    }

    fn role_prelude(self) -> &'static str {
        match self {
            ShaderRole::Color => include_str!("wgsl/color.wgsl"),
            ShaderRole::Mix => include_str!("wgsl/mix.wgsl"),
            ShaderRole::Texture => include_str!("wgsl/texture.wgsl"),
            ShaderRole::Mask => include_str!("wgsl/mask.wgsl"),
            ShaderRole::LocalMotion => include_str!("wgsl/local_motion.wgsl"),
            ShaderRole::TransitionalMotion => include_str!("wgsl/transitional_motion.wgsl"),
        }
    }
}

impl std::fmt::Display for ShaderRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

pub const VERTEX_ENTRY: &str = "kg_vs";
pub const FRAGMENT_ENTRY: &str = "kg_fs";
pub const COMPUTE_ENTRY: &str = "kg_cs";
/// Largest packed argument value, in bytes.
pub const MAX_ARGS_BYTES: u32 = 16384;
/// Motion displacement slots; slot 0 is the identity.
pub const MOTION_SLOTS: u32 = 256;
/// Size of the displacement rows buffer (`MOTION_SLOTS` × 2 × vec4).
pub const TRANSFORMS_BYTES: u64 = 8192;
/// Size of the motion state buffer (`MOTION_SLOTS` × vec4).
pub const STATES_BYTES: u64 = 4096;

const COMMON: &str = include_str!("wgsl/common.wgsl");
const RASTER: &str = include_str!("wgsl/raster.wgsl");
const MOTION: &str = include_str!("wgsl/motion.wgsl");

/// The WGSL that precedes a user shader of `role`.
pub fn prelude(role: ShaderRole) -> String {
    let stage = if role.is_compute() { MOTION } else { RASTER };
    [COMMON, stage, role.role_prelude()].concat()
}

/// The full shader module: prelude, a newline, then the user's source.
pub fn compose(role: ShaderRole, source: &str) -> String {
    let mut out = prelude(role);
    out.push('\n');
    out.push_str(source);
    out
}

/// Lines of `compose(role, _)` before the user's first line.
pub fn prelude_lines(role: ShaderRole) -> usize {
    prelude(role).matches('\n').count().saturating_add(1)
}
