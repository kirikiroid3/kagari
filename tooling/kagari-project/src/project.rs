use std::path::{Path, PathBuf};

use kagari_lang::{FilePath, Program, RootDatabase};

use crate::error::ProjectError;
use crate::fonts::system_font_dirs;
use crate::manifest::{Manifest, parse_manifest};
use crate::target::Target;
use crate::{MANIFEST_FILE, TOOLCHAIN_VERSION};

/// A loaded project: its manifest and the directory holding it.
#[derive(Clone, Debug)]
pub struct Project {
    /// The canonical directory of `kagari.toml`; manifest paths are relative to it.
    pub root: PathBuf,
    /// The manifest file, inside `root`.
    pub manifest_path: PathBuf,
    pub manifest: Manifest,
}

impl Project {
    /// Reads and validates the manifest at `manifest_path` and checks the toolchain version.
    pub fn load(manifest_path: &Path) -> Result<Project, ProjectError> {
        let io = |error| ProjectError::Io {
            path: manifest_path.to_path_buf(),
            error,
        };
        let text = std::fs::read_to_string(manifest_path).map_err(io)?;
        let manifest = parse_manifest(&text).map_err(|message| ProjectError::Manifest {
            path: manifest_path.to_path_buf(),
            message,
        })?;
        let dir = match manifest_path.parent() {
            Some(dir) if !dir.as_os_str().is_empty() => dir,
            _ => Path::new("."),
        };
        let root = std::fs::canonicalize(dir).map_err(|error| ProjectError::Io {
            path: dir.to_path_buf(),
            error,
        })?;
        let manifest_path = match manifest_path.file_name() {
            Some(name) => root.join(name),
            None => root.join(MANIFEST_FILE),
        };
        let running = semver::Version::parse(TOOLCHAIN_VERSION).map_err(|error| {
            ProjectError::Manifest {
                path: manifest_path.clone(),
                message: format!("invalid toolchain version `{TOOLCHAIN_VERSION}`: {error}"),
            }
        })?;
        if !manifest.toolchain.matches(&running) {
            return Err(ProjectError::Toolchain {
                path: manifest_path,
                required: manifest.toolchain,
                running,
            });
        }
        Ok(Project {
            root,
            manifest_path,
            manifest,
        })
    }

    /// Loads the `kagari.toml` in `start` or the nearest parent directory that has one.
    pub fn find(start: &Path) -> Result<Project, ProjectError> {
        for dir in start.ancestors() {
            let candidate = dir.join(MANIFEST_FILE);
            if candidate.is_file() {
                return Project::load(&candidate);
            }
        }
        Err(ProjectError::NotFound {
            start: start.to_path_buf(),
        })
    }

    /// The root source file.
    pub fn entry_path(&self) -> PathBuf {
        self.root.join(&self.manifest.entry)
    }

    /// The directory `asset("…")` paths are relative to.
    pub fn asset_dir(&self) -> PathBuf {
        self.root.join(&self.manifest.asset_dir)
    }

    /// Where `kagari build` writes the output of `target`.
    pub fn out_dir(&self, target: Target) -> PathBuf {
        self.root.join(&self.manifest.out_dir).join(target.as_str())
    }

    /// The project's program in `db`. `system_font("…")` searches this machine's system font
    /// directories unless the manifest forbids system fonts.
    pub fn program(&self, db: &RootDatabase) -> Program {
        let font_dirs = if self.manifest.options.system_fonts {
            system_font_dirs().into_iter().map(FilePath::disk).collect()
        } else {
            Vec::new()
        };
        db.program_with_assets(
            FilePath::disk(self.entry_path()),
            FilePath::disk(self.asset_dir()),
            font_dirs,
            self.manifest.options,
        )
    }
}
