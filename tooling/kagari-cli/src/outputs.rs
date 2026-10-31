//! The files the CLI writes into a project's output directory, and their removal.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use kagari_lang::kagari_debug;

use crate::failure::{Failure, io};

/// The bytecode file in each target directory.
pub const BYTECODE_FILE: &str = "game.kagari";
/// The directory of the output directory that `kagari release` writes into.
pub const RELEASE_DIR: &str = "release";
/// The extension of the images `kagari release` writes.
pub const IMAGE_EXTENSION: &str = "iso";

/// Whether `path` names a file a game needs at run time: the bytecode or an asset archive.
fn is_game_file(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == BYTECODE_FILE)
        || path
            .extension()
            .is_some_and(|ext| ext == kagari_archive::archive::ARCHIVE_EXTENSION)
}

/// Whether `path` names a file `kagari build` writes into a target directory: a game file or
/// the debug artifact.
fn is_target_output(path: &Path) -> bool {
    is_game_file(path)
        || path
            .extension()
            .is_some_and(|ext| ext == kagari_debug::EXTENSION)
}

/// Deletes the build outputs in the target directory `dir` (other files are left alone);
/// returns how many. A missing directory has none.
pub fn remove_target_outputs(dir: &Path) -> Result<usize, Failure> {
    remove_matching(dir, is_target_output)
}

/// The bytecode and the archives in the target directory `dir`, sorted.
pub fn game_files(dir: &Path) -> Result<Vec<PathBuf>, Failure> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|error| io(dir, error))? {
        let path = entry.map_err(|error| io(dir, error))?.path();
        if is_game_file(&path) && path.is_file() {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

/// The directory of the images `kagari release` writes for `platform` in the output
/// directory `build_dir`.
pub fn release_dir(build_dir: &Path, platform: &str) -> PathBuf {
    build_dir.join("release").join(platform)
}

/// Deletes the images in the release directory `dir`; returns how many.
pub fn remove_images(dir: &Path) -> Result<usize, Failure> {
    remove_matching(dir, |path| {
        path.extension().is_some_and(|ext| ext == IMAGE_EXTENSION)
    })
}

/// Deletes the `.kagari-cache` files directly in `dir`; returns how many.
pub fn remove_caches(dir: &Path) -> Result<usize, Failure> {
    remove_matching(dir, |path| {
        path.extension()
            .is_some_and(|ext| ext == kagari_debug::EXTENSION)
    })
}

fn remove_matching(dir: &Path, matches: impl Fn(&Path) -> bool) -> Result<usize, Failure> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(io(dir, error)),
    };
    let mut removed: usize = 0;
    for entry in entries {
        let path = entry.map_err(|error| io(dir, error))?.path();
        if matches(&path) && path.is_file() {
            std::fs::remove_file(&path).map_err(|error| io(&path, error))?;
            removed = removed.saturating_add(1);
        }
    }
    Ok(removed)
}

/// Removes `dir` when it exists and is empty.
pub fn remove_dir_if_empty(dir: &Path) -> Result<bool, Failure> {
    let mut entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(io(dir, error)),
    };
    if entries.next().is_some() {
        return Ok(false);
    }
    std::fs::remove_dir(dir).map_err(|error| io(dir, error))?;
    Ok(true)
}
