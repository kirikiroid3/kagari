//! Runtime profiles: named sets of decoders a runtime provides.

/// A runtime profile that `build.runtime-compatibility` can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RuntimeProfile {
    /// The native desktop runtime, version 0. Frozen: what it accepts never changes.
    NativeV0,
    /// The native desktop runtime as it is developed. New formats and features land here, never
    /// in `NativeV0`, so what it accepts can change between releases.
    NativeUnstable,
}

impl RuntimeProfile {
    /// Every profile, in declaration order.
    pub const ALL: &[RuntimeProfile] = &[RuntimeProfile::NativeV0, RuntimeProfile::NativeUnstable];

    /// The manifest name of the profile.
    pub fn as_str(self) -> &'static str {
        match self {
            RuntimeProfile::NativeV0 => "native_v0",
            RuntimeProfile::NativeUnstable => "native_unstable",
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
