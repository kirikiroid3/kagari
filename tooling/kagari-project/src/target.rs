/// A build target: the engine major version the bytecode is built for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Target {
    V0,
}

impl Target {
    /// Every supported target.
    pub const ALL: &[Target] = &[Target::V0];

    /// The target's name in `kagari.toml` and on the command line.
    pub fn as_str(self) -> &'static str {
        match self {
            Target::V0 => "v0",
        }
    }

    /// The target named `name`.
    pub fn parse(name: &str) -> Option<Target> {
        Target::ALL
            .iter()
            .copied()
            .find(|target| target.as_str() == name)
    }
}
