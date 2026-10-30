//! The `.kiarchive` files of a game directory, opened on first use.

use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use super::{ARCHIVE_EXTENSION, Error, PatchedArchive, archive_stem, open_chain};

/// The archives of a game directory, keyed by [`archive_stem`] (the grouping `kagari build`
/// writes). A missing archive is remembered as `None` and reported once.
pub struct ArchiveDir {
    dir: PathBuf,
    open: HashMap<String, Option<PatchedArchive<File>>>,
}

impl ArchiveDir {
    pub fn new(dir: &Path) -> ArchiveDir {
        ArchiveDir {
            dir: dir.to_path_buf(),
            open: HashMap::new(),
        }
    }

    /// The bytes of the asset at a bytecode asset path; `Ok(None)` when its archive or its
    /// entry is absent. `missing` is called with the archive's path the first time it is found
    /// absent.
    pub fn read(&mut self, path: &str, missing: impl FnOnce(&Path)) -> io::Result<Option<Vec<u8>>> {
        let stem = archive_stem(path);
        let file = self.dir.join(format!("{stem}.{ARCHIVE_EXTENSION}"));
        if !self.open.contains_key(stem) {
            let archive = match open_chain(&file) {
                Ok(archive) => Some(archive),
                Err(Error::Io(e)) if e.kind() == io::ErrorKind::NotFound => {
                    missing(&file);
                    None
                }
                Err(e) => return Err(invalid(&file, &e)),
            };
            self.open.insert(stem.to_owned(), archive);
        }
        let Some(Some(archive)) = self.open.get_mut(stem) else {
            return Ok(None);
        };
        match archive.read(path) {
            Ok(bytes) => Ok(bytes),
            Err(Error::Io(e)) => Err(e),
            Err(e) => Err(invalid(&file, &e)),
        }
    }
}

fn invalid(file: &Path, error: &Error) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("{}: {error}", file.display()),
    )
}
