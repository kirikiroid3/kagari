use std::path::Path;

use kagari_project::Project;

use crate::failure::{Failure, io};

/// The project of `--manifest-path`, or else the nearest one from the current directory.
pub fn load(manifest_path: Option<&Path>) -> Result<Project, Failure> {
    match manifest_path {
        Some(path) => Ok(Project::load(path)?),
        None => {
            let cwd = std::env::current_dir().map_err(|error| io(Path::new("."), error))?;
            Ok(Project::find(&cwd)?)
        }
    }
}

/// `path` relative to `base` when it lies under it, with `/` separators.
pub fn shown(path: &Path, base: &Path) -> String {
    kagari_lang::FilePath::disk(path).display(Some(base))
}
