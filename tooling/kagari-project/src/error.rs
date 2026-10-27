use std::fmt;
use std::io;
use std::path::PathBuf;

/// Why a project could not be loaded.
#[derive(Debug)]
pub enum ProjectError {
    /// No `kagari.toml` in `start` or any parent directory.
    NotFound { start: PathBuf },
    /// Reading `path` failed.
    Io { path: PathBuf, error: io::Error },
    /// The manifest at `path` is invalid.
    Manifest { path: PathBuf, message: String },
    /// The manifest at `path` requires another toolchain version.
    Toolchain {
        path: PathBuf,
        required: semver::VersionReq,
        running: semver::Version,
    },
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProjectError::NotFound { start } => write!(
                f,
                "could not find `kagari.toml` in `{}` or any parent directory",
                start.display()
            ),
            ProjectError::Io { path, error } => write!(f, "{}: {error}", path.display()),
            ProjectError::Manifest { path, message } => write!(f, "{}: {message}", path.display()),
            ProjectError::Toolchain {
                path,
                required,
                running,
            } => write!(
                f,
                "{}: this project requires kagari {required}, but this is kagari {running}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for ProjectError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ProjectError::Io { error, .. } => Some(error),
            _ => None,
        }
    }
}
