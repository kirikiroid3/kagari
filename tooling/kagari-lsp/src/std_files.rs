//! Std files live inside the compiler; the editor needs them on disk to open a definition in
//! them, so they are written to a per-version directory under the system's temp directory.

use std::path::PathBuf;

use kagari_project::TOOLCHAIN_VERSION;

/// The on-disk copy of std file `name` (`/`-separated, relative to the std directory), written
/// when missing. `None` when `name` is no std file or the file cannot be written.
pub fn materialize(name: &str) -> Option<PathBuf> {
    let text = kagari_std::file(name)?;
    let mut path = std::env::temp_dir().join(format!("kagari-std-{TOOLCHAIN_VERSION}"));
    path.extend(name.split('/').filter(|part| !part.is_empty()));
    if !path.is_file() {
        std::fs::create_dir_all(path.parent()?).ok()?;
        std::fs::write(&path, text).ok()?;
    }
    Some(path)
}
