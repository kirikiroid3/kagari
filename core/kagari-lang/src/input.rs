//! Salsa inputs: source files and the program entry, plus `src` path resolution.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

/// Identity of a source file: a bundled std file or a file on disk.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FilePath {
    /// `std/<name>`, embedded in `kagari-std`. The string uses `/` separators.
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

    /// The file extension, without the dot.
    pub fn extension(&self) -> Option<&str> {
        match self {
            FilePath::Std(name) => {
                let file = name.rsplit('/').next().unwrap_or(name);
                file.rsplit_once('.').map(|(_, ext)| ext)
            }
            FilePath::Disk(path) => path.extension().and_then(|ext| ext.to_str()),
        }
    }

    /// The std file name (`prelude.kix`, `sub/x.kix`) of a std path.
    pub fn std_name(&self) -> Option<&str> {
        match self {
            FilePath::Std(name) => Some(name),
            FilePath::Disk(_) => None,
        }
    }
}

/// Why a `src` attribute could not be resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResolveError {
    Empty,
    Absolute,
    EscapesStd,
}

impl ResolveError {
    pub fn message(self) -> &'static str {
        match self {
            ResolveError::Empty => "`src` is empty",
            ResolveError::Absolute => "`src` must be a relative path",
            ResolveError::EscapesStd => "`src` escapes the std directory",
        }
    }
}

/// Resolve a `src` attribute written in `from`.
pub fn resolve_src(from: &FilePath, src: &str) -> Result<FilePath, ResolveError> {
    if src.is_empty() {
        return Err(ResolveError::Empty);
    }
    if src.starts_with('/') || src.starts_with('\\') || Path::new(src).is_absolute() {
        return Err(ResolveError::Absolute);
    }
    if let Some(rest) = src.strip_prefix("std/") {
        return normalize_std(&[], rest).map(FilePath::Std);
    }
    match from {
        FilePath::Disk(path) => {
            let parent = path.parent().unwrap_or(path);
            Ok(FilePath::disk(parent.join(src)))
        }
        FilePath::Std(name) => {
            let mut dir: Vec<&str> = name.split('/').collect();
            dir.pop();
            normalize_std(&dir, src).map(FilePath::Std)
        }
    }
}

fn normalize_std(base: &[&str], rel: &str) -> Result<String, ResolveError> {
    let mut parts: Vec<&str> = base.to_vec();
    for part in rel.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(ResolveError::EscapesStd);
                }
            }
            part => parts.push(part),
        }
    }
    if parts.is_empty() {
        return Err(ResolveError::Empty);
    }
    Ok(parts.join("/"))
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

/// A program rooted at its entry `.kis` file.
#[salsa::input(debug)]
pub struct Program {
    pub entry: SourceFile,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_paths() {
        let main = FilePath::disk("/g/script/main.kis");
        assert_eq!(
            resolve_src(&main, "../ext/a.kix"),
            Ok(FilePath::Disk("/g/ext/a.kix".into()))
        );
        assert_eq!(
            resolve_src(&main, "std/ui.kix"),
            Ok(FilePath::Std("ui.kix".into()))
        );
        assert_eq!(
            resolve_src(&main, "std/../x"),
            Err(ResolveError::EscapesStd)
        );
        assert_eq!(resolve_src(&main, ""), Err(ResolveError::Empty));
        assert_eq!(resolve_src(&main, "/abs"), Err(ResolveError::Absolute));
        let std = FilePath::Std("sub/a.kix".into());
        assert_eq!(
            resolve_src(&std, "b.kix"),
            Ok(FilePath::Std("sub/b.kix".into()))
        );
        assert_eq!(
            resolve_src(&std, "../../b.kix"),
            Err(ResolveError::EscapesStd)
        );
        assert_eq!(
            FilePath::disk("/g/script/main.kis").display(Some(Path::new("/g"))),
            "script/main.kis"
        );
        assert_eq!(FilePath::Std("ui.kix".into()).extension(), Some("kix"));
    }
}
