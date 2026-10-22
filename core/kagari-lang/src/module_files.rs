//! File lookup for `mod foo;` (the Rust 2018 rule).

use crate::db::Db;
use crate::hir::Name;
use crate::input::{FilePath, SourceFile};

/// The outcome of looking up the file of `mod foo;`. Candidate names are shown relative to the
/// declaring file's directory, with `/` separators.
pub(crate) enum ModFile {
    Found(SourceFile),
    NotFound(Vec<String>),
    Ambiguous(Vec<String>),
}

/// The directory holding the files of modules declared in `file` inside the inline modules
/// `inline_path`, plus that directory relative to the declaring file's directory.
fn child_dir(
    db: &dyn Db,
    file: SourceFile,
    is_crate_root: bool,
    inline_path: &[Name],
) -> (FilePath, String) {
    let path = file.path(db);
    let is_mod_file = matches!(path.file_name(), Some("mod.kis" | "mod.kix"));
    let mut relative = String::new();
    if !is_crate_root && !is_mod_file {
        relative.push_str(path.file_stem().unwrap_or_default());
        relative.push('/');
    }
    for name in inline_path {
        relative.push_str(name);
        relative.push('/');
    }
    let dir = path.parent_dir_join(if relative.is_empty() { "." } else { &relative });
    (dir, relative)
}

pub(crate) fn lookup_mod_file(
    db: &dyn Db,
    file: SourceFile,
    is_crate_root: bool,
    inline_path: &[Name],
    name: &str,
) -> ModFile {
    let (dir, relative) = child_dir(db, file, is_crate_root, inline_path);
    let candidates = [
        format!("{name}.kis"),
        format!("{name}.kix"),
        format!("{name}/mod.kis"),
        format!("{name}/mod.kix"),
    ];
    let mut found = Vec::new();
    for candidate in &candidates {
        let source = db.source_file(dir.join(candidate));
        if source.text(db).is_some() {
            found.push((source, format!("{relative}{candidate}")));
        }
    }
    match found.len() {
        0 => ModFile::NotFound(
            candidates
                .iter()
                .map(|candidate| format!("{relative}{candidate}"))
                .collect(),
        ),
        1 => match found.pop() {
            Some((source, _)) => ModFile::Found(source),
            None => ModFile::NotFound(Vec::new()),
        },
        _ => ModFile::Ambiguous(found.into_iter().map(|(_, shown)| shown).collect()),
    }
}
