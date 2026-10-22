//! Salsa inputs: source files, asset files and the program root.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

/// Identity of a file: a bundled std file or a file on disk. Also names directories
/// (see [`FilePath::join`]).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FilePath {
    /// `std/<name>`, embedded in `kagari-std`. The string uses `/` separators and is relative
    /// to the std directory.
    Std(String),
    /// An absolute, lexically normalized path on disk.
    Disk(PathBuf),
}

impl FilePath {
    /// A disk path with `.` dropped and `..` folded lexically. Callers pass absolute paths.
    pub fn disk(path: impl Into<PathBuf>) -> FilePath {
        FilePath::Disk(normalize(&path.into()))
    }

    /// Human-readable path: `std/<name>` for std files; disk paths relative to `base`
    /// when they lie under it, always with `/` separators.
    pub fn display(&self, base: Option<&Path>) -> String {
        match self {
            FilePath::Std(name) => format!("std/{name}"),
            FilePath::Disk(path) => {
                let shown = base
                    .and_then(|base| path.strip_prefix(base).ok())
                    .unwrap_or(path);
                let parts: Vec<String> = shown
                    .components()
                    .filter_map(|component| match component {
                        Component::RootDir => Some(String::new()),
                        Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
                        Component::Prefix(prefix) => {
                            Some(prefix.as_os_str().to_string_lossy().into_owned())
                        }
                        Component::CurDir | Component::ParentDir => None,
                    })
                    .collect();
                let joined = parts.join("/");
                if joined.is_empty() {
                    "/".to_owned()
                } else {
                    joined
                }
            }
        }
    }

    /// The file name (last component).
    pub fn file_name(&self) -> Option<&str> {
        match self {
            FilePath::Std(name) => name.rsplit('/').next().filter(|name| !name.is_empty()),
            FilePath::Disk(path) => path.file_name().and_then(|name| name.to_str()),
        }
    }

    /// The file name without its extension.
    pub fn file_stem(&self) -> Option<&str> {
        let name = self.file_name()?;
        Some(name.rsplit_once('.').map_or(name, |(stem, _)| stem))
    }

    /// The file extension, without the dot.
    pub fn extension(&self) -> Option<&str> {
        self.file_name()?.rsplit_once('.').map(|(_, ext)| ext)
    }

    /// `rel` (a `/`-separated relative path) joined to this path, lexically normalized.
    pub fn join(&self, rel: &str) -> FilePath {
        match self {
            FilePath::Std(name) => {
                let base: Vec<&str> = name.split('/').filter(|part| !part.is_empty()).collect();
                FilePath::Std(normalize_std(&base, rel))
            }
            FilePath::Disk(path) => FilePath::disk(path.join(rel)),
        }
    }

    /// `rel` joined to the directory containing this file, lexically normalized.
    pub fn parent_dir_join(&self, rel: &str) -> FilePath {
        match self {
            FilePath::Std(name) => {
                let mut base: Vec<&str> = name.split('/').filter(|part| !part.is_empty()).collect();
                base.pop();
                FilePath::Std(normalize_std(&base, rel))
            }
            FilePath::Disk(path) => {
                let parent = path.parent().unwrap_or(path);
                FilePath::disk(parent.join(rel))
            }
        }
    }
}

fn normalize_std(base: &[&str], rel: &str) -> String {
    let mut parts: Vec<&str> = base.to_vec();
    for part in rel.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    parts.join("/")
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push(component);
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// One source file. `text` is `None` when the file is missing or unreadable.
#[salsa::input(debug)]
pub struct SourceFile {
    #[returns(ref)]
    pub path: FilePath,
    #[returns(ref)]
    pub text: Option<Arc<str>>,
}

/// A file named by an asset path; only its existence matters to the front end.
#[salsa::input(debug)]
pub struct AssetFile {
    #[returns(ref)]
    pub path: FilePath,
    pub exists: bool,
}

/// A program rooted at its root `.kis`/`.kix` file.
#[salsa::input(debug)]
pub struct Program {
    pub root: SourceFile,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_paths() {
        let main = FilePath::disk("/g/script/main.kix");
        assert_eq!(
            main.parent_dir_join("../ext/a.png"),
            FilePath::Disk("/g/ext/a.png".into())
        );
        assert_eq!(
            FilePath::Std("system.kis".into()).parent_dir_join("system/nav.kis"),
            FilePath::Std("system/nav.kis".into())
        );
        assert_eq!(
            FilePath::Std("narrative".into()).join("./text.kis"),
            FilePath::Std("narrative/text.kis".into())
        );
        assert_eq!(main.file_stem(), Some("main"));
        assert_eq!(
            FilePath::disk("/g/script/main.kis").display(Some(Path::new("/g"))),
            "script/main.kis"
        );
    }
}
