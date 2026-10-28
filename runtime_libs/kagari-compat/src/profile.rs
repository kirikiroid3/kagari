//! Runtime profiles: named sets of decoders a runtime provides.

/// A runtime profile that `build.runtime-compatibility` can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RuntimeProfile {
    /// The native desktop runtime, version 0.
    NativeV0,
}

impl RuntimeProfile {
    /// Every profile, in declaration order.
    pub const ALL: &[RuntimeProfile] = &[RuntimeProfile::NativeV0];

    /// The manifest name of the profile.
    pub fn as_str(self) -> &'static str {
        match self {
            RuntimeProfile::NativeV0 => "native_v0",
        }
    }

    /// The profile named exactly `name`.
    pub fn parse(name: &str) -> Option<RuntimeProfile> {
        RuntimeProfile::ALL
            .iter()
            .copied()
            .find(|profile| profile.as_str() == name)
    }
}
