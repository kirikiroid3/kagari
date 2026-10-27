//! The salsa database and its file system access.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use rustc_hash::FxHashMap;
use salsa::Setter;

use crate::input::{AssetFile, CompileOptions, FilePath, Program, SourceFile};

/// Reads source files and checks asset files, on disk or in memory (tests, the LSP).
pub trait FileSystem: Send + Sync {
    fn read(&self, path: &Path) -> Option<String>;
    fn exists(&self, path: &Path) -> bool;
}

/// The real file system.
pub struct OsFileSystem;

impl FileSystem for OsFileSystem {
    fn read(&self, path: &Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }

    fn exists(&self, path: &Path) -> bool {
        path.is_file()
    }
}

/// An in-memory file system: path → contents.
#[derive(Clone, Debug, Default)]
pub struct MemoryFileSystem(pub FxHashMap<PathBuf, String>);

impl FileSystem for MemoryFileSystem {
    fn read(&self, path: &Path) -> Option<String> {
        self.0.get(path).cloned()
    }

    fn exists(&self, path: &Path) -> bool {
        self.0.contains_key(path)
    }
}

#[salsa::db]
pub trait Db: salsa::Database {
    /// The input for the source file at `path`, created (and read) on first use.
    fn source_file(&self, path: FilePath) -> SourceFile;
    /// The input for the asset file at `path`, created (and checked) on first use.
    fn asset_file(&self, path: FilePath) -> AssetFile;
}

#[salsa::db]
#[derive(Clone)]
pub struct RootDatabase {
    storage: salsa::Storage<Self>,
    files: Arc<Mutex<FxHashMap<FilePath, SourceFile>>>,
    assets: Arc<Mutex<FxHashMap<FilePath, AssetFile>>>,
    fs: Arc<dyn FileSystem>,
}

impl RootDatabase {
    pub fn new(fs: Arc<dyn FileSystem>) -> Self {
        RootDatabase {
            storage: salsa::Storage::default(),
            files: Arc::default(),
            assets: Arc::default(),
            fs,
        }
    }

    /// A database that reports every salsa event to `callback` (tests of incrementality).
    pub fn with_event_callback(
        fs: Arc<dyn FileSystem>,
        callback: Box<dyn Fn(salsa::Event) + Send + Sync>,
    ) -> Self {
        RootDatabase {
            storage: salsa::Storage::new(Some(callback)),
            files: Arc::default(),
            assets: Arc::default(),
            fs,
        }
    }

    /// The program rooted at `root`, with default options.
    pub fn program(&self, root: FilePath) -> Program {
        self.program_with(root, CompileOptions::default())
    }

    /// The program rooted at `root`, compiled with `options`.
    pub fn program_with(&self, root: FilePath, options: CompileOptions) -> Program {
        Program::new(self, self.source_file(root), options)
    }

    /// Replace a file's text (`None` = missing). Creates the input if needed.
    pub fn set_file_text(&mut self, path: FilePath, text: Option<String>) {
        let file = self.source_file(path);
        file.set_text(self).to(text.map(Arc::from));
    }

    /// Record whether an asset file exists. Creates the input if needed.
    pub fn set_asset_exists(&mut self, path: FilePath, exists: bool) {
        let asset = self.asset_file(path);
        asset.set_exists(self).to(exists);
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

    fn asset_file(&self, path: FilePath) -> AssetFile {
        let mut assets = self.assets.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(asset) = assets.get(&path) {
            return *asset;
        }
        let exists = match &path {
            FilePath::Std(_) => false,
            FilePath::Disk(disk) => self.fs.exists(disk),
        };
        let asset = AssetFile::new(self, path.clone(), exists);
        assets.insert(path, asset);
        asset
    }
}
