//! `kagari clean`: deletes what `check` and `build` wrote into the output directory.

use std::path::Path;
use std::process::ExitCode;

use kagari_project::Target;

use crate::failure::Failure;
use crate::outputs::{remove_caches, remove_dir_if_empty, remove_target_outputs};
use crate::workspace::{self, shown};

/// Removes, in `build.out-dir`, the analysis cache and, in the directory of every target
/// (listed in the manifest or not), the bytecode, the archives and the debug artifact; then
/// the directories left empty. Other files stay.
pub fn run(manifest_path: Option<&Path>) -> Result<ExitCode, Failure> {
    let project = workspace::load(manifest_path)?;
    let build_dir = project.build_dir();
    let mut removed = remove_caches(&build_dir)?;
    for target in Target::ALL {
        let dir = project.out_dir(*target);
        removed = removed.saturating_add(remove_target_outputs(&dir)?);
        remove_dir_if_empty(&dir)?;
    }
    remove_dir_if_empty(&build_dir)?;
    let files = if removed == 1 { "file" } else { "files" };
    println!(
        "removed {removed} {files} from {}",
        shown(&build_dir, &project.root)
    );
    Ok(ExitCode::SUCCESS)
}
