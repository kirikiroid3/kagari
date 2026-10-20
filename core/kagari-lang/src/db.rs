//! The salsa database and its file system access.

use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use rustc_hash::FxHashMap;
use salsa::Setter;

use crate::input::{FilePath, Program, SourceFile};

/// Reads source files from disk (or from memory, in tests and the LSP).
pub trait FileSystem: Send + Sync {
    fn read(&self, path: &Path) -> Option<String>;
}

/// The real file system.
pub struct OsFileSystem;

impl FileSystem for OsFileSystem {
    fn read(&self, path: &Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }
}

#[salsa::db]
pub trait Db: salsa::Database {
    /// The input for `path`, created (and read) on first use.
    fn source_file(&self, path: FilePath) -> SourceFile;
}

#[salsa::db]
#[derive(Clone)]
pub struct RootDatabase {
    storage: salsa::Storage<Self>,
    files: Arc<Mutex<FxHashMap<FilePath, SourceFile>>>,
    fs: Arc<dyn FileSystem>,
}

impl RootDatabase {
    pub fn new(fs: Arc<dyn FileSystem>) -> Self {
        RootDatabase {
            storage: salsa::Storage::default(),
            files: Arc::default(),
            fs,
        }
    }

    /// The program rooted at `entry`.
    pub fn program(&self, entry: FilePath) -> Program {
        Program::new(self, self.source_file(entry))
    }

    /// Replace a file's text (`None` = missing). Creates the input if needed.
    pub fn set_file_text(&mut self, path: FilePath, text: Option<String>) {
        let file = self.source_file(path);
        file.set_text(self).to(text.map(Arc::from));
    }
}

#[salsa::db]
impl salsa::Database for RootDatabase {}

#[salsa::db]
impl Db for RootDatabase {
    fn source_file(&self, path: FilePath) -> SourceFile {
        let mut files = self.files.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(file) = files.get(&path) {
            return *file;
        }
        let text: Option<Arc<str>> = match &path {
            FilePath::Std(name) => kagari_std::file(name).map(Arc::from),
            FilePath::Disk(disk) => self.fs.read(disk).map(Arc::from),
        };
        let file = SourceFile::new(self, path.clone(), text);
        files.insert(path, file);
        file
    }
}
