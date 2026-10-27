//! Where asset files are: `asset("…")` paths in the program's asset directory,
//! `system_font("…")` paths in the system font directories, and the path each file has in
//! the bytecode.

use std::path::Component;

use crate::db::Db;
use crate::input::{FilePath, Program};

/// The top-level bytecode directory of system fonts. `asset("…")` paths may not enter the
/// directory of this name, so an asset and a system font never share a bytecode path.
pub const SYSTEM_FONTS_DIR: &str = "system-fonts";

/// `rel` joined to `dir`, when the result lies strictly inside `dir`.
pub(crate) fn inside(dir: &FilePath, rel: &str) -> Option<FilePath> {
    let file = dir.join(rel);
    match (&file, dir) {
        (FilePath::Disk(path), FilePath::Disk(root))
            if path.as_ref() != root.as_ref() && path.starts_with(root) =>
        {
            Some(file)
        }
        _ => None,
    }
}

/// Whether the relative path `rel` names a file below the directory it is joined to: not
/// absolute, not empty, and never above that directory.
pub(crate) fn stays_inside(rel: &str) -> bool {
    if rel.starts_with(['/', '\\']) {
        return false;
    }
    let mut depth = 0usize;
    for (index, part) in rel.split(['/', '\\']).enumerate() {
        match part {
            "" | "." => {}
            ".." => match depth.checked_sub(1) {
                Some(up) => depth = up,
                None => return false,
            },
            // A drive or URL prefix (`C:`) makes the path absolute.
            part if index == 0 && part.contains(':') => return false,
            _ => depth = depth.saturating_add(1),
        }
    }
    depth > 0
}

/// Whether `file`, inside the asset directory `root`, lies in its reserved `system-fonts`
/// directory.
pub(crate) fn is_reserved(file: &FilePath, root: &FilePath) -> bool {
    let (FilePath::Disk(path), FilePath::Disk(root)) = (file, root) else {
        return false;
    };
    path.strip_prefix(root)
        .ok()
        .and_then(|rel| rel.components().next())
        .is_some_and(|first| first == Component::Normal(SYSTEM_FONTS_DIR.as_ref()))
}

/// The file `system_font(rel)` names: `rel` in the first system font directory that has it.
/// `rel` must satisfy [`stays_inside`].
pub(crate) fn system_font_file(db: &dyn Db, program: Program, rel: &str) -> Option<FilePath> {
    program
        .font_dirs(db)
        .iter()
        .filter_map(|dir| inside(dir, rel))
        .find(|file| *db.asset_file(file.clone()).exists(db))
}

/// The path of the asset `file` in the bytecode, `/`-separated: relative to the asset
/// directory, or `system-fonts/` and the path relative to the system font directory that
/// holds it.
pub(crate) fn bytecode_path(db: &dyn Db, program: Program, file: &FilePath) -> String {
    let FilePath::Disk(path) = file else {
        return file.display(None);
    };
    if let FilePath::Disk(root) = program.asset_root(db)
        && path.starts_with(root)
    {
        return file.display(Some(root));
    }
    for dir in program.font_dirs(db).iter() {
        if let FilePath::Disk(dir) = dir
            && path.starts_with(dir)
        {
            return format!("{SYSTEM_FONTS_DIR}/{}", file.display(Some(dir)));
        }
    }
    file.display(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_stay_inside() {
        assert!(stays_inside("msgothic.ttc"));
        assert!(stays_inside("truetype/dejavu/../dejavu/DejaVuSans.ttf"));
        assert!(stays_inside("a\\b.ttf"));
        assert!(!stays_inside(""));
        assert!(!stays_inside("."));
        assert!(!stays_inside("a/.."));
        assert!(!stays_inside("../a.ttf"));
        assert!(!stays_inside("a/../../b.ttf"));
        assert!(!stays_inside("/usr/share/fonts/a.ttf"));
        assert!(!stays_inside("\\Windows\\Fonts\\a.ttf"));
        assert!(!stays_inside("C:/Windows/Fonts/a.ttf"));
    }

    #[test]
    fn the_system_fonts_directory_is_reserved() {
        let root = FilePath::disk("/game/assets");
        assert!(is_reserved(&root.join("system-fonts/a.ttf"), &root));
        assert!(!is_reserved(&root.join("fonts/system-fonts/a.ttf"), &root));
        assert!(!is_reserved(&root.join("system-fonts.ttf"), &root));
    }
}
