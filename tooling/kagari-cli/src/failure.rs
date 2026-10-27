use std::io;
use std::path::Path;
use std::process::ExitCode;

use kagari_project::ProjectError;

/// A command that cannot run: a usage, IO, manifest or toolchain error, reported with exit
/// code 2.
pub struct Failure(pub String);

impl From<ProjectError> for Failure {
    fn from(error: ProjectError) -> Failure {
        Failure(error.to_string())
    }
}

/// `{path}: {error}`.
pub fn io(path: &Path, error: io::Error) -> Failure {
    Failure(format!("{}: {error}", path.display()))
}

/// Exit code 1 when the command found problems (diagnostics, unformatted files), else 0.
pub fn exit(failed: bool) -> ExitCode {
    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
