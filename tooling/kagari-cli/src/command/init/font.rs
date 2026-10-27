//! The default font of a new project: a font installed in this machine's system font
//! directories, named as `system_font("…")` takes it.

use std::path::Path;

use kagari_project::system_font_dirs;

/// Font files preferred as the default, by file name (ASCII case-insensitive), best first:
/// Japanese gothic faces, then common sans-serif faces.
const PREFERRED: &[&str] = &[
    "YuGothM.ttc",
    "meiryo.ttc",
    "msgothic.ttc",
    "ヒラギノ角ゴシック W3.ttc",
    "NotoSansCJK-Regular.ttc",
    "NotoSansCJKjp-Regular.otf",
    "NotoSansJP-Regular.otf",
    "segoeui.ttf",
    "arial.ttf",
    "Helvetica.ttc",
    "NotoSans-Regular.ttf",
    "DejaVuSans.ttf",
    "LiberationSans-Regular.ttf",
];

/// The extensions of font files.
const EXTENSIONS: &[&str] = &["ttf", "otf", "ttc", "otc"];

/// How deep below a font directory fonts are looked for; it also stops symlink cycles.
const MAX_DEPTH: usize = 6;

/// The path of the default font relative to its system font directory, `/`-separated: the
/// first [`PREFERRED`] font found, or else the first font file of the first directory that has
/// one, in path order. `None` when no font is installed.
pub fn default_font() -> Option<String> {
    let mut fonts: Vec<String> = Vec::new();
    for dir in system_font_dirs() {
        let mut found = Vec::new();
        collect(&dir, "", 0, &mut found);
        found.sort();
        fonts.extend(found);
    }
    let file_name = |path: &String| path.rsplit('/').next().unwrap_or(path).to_owned();
    PREFERRED
        .iter()
        .find_map(|name| {
            fonts
                .iter()
                .find(|path| file_name(path).eq_ignore_ascii_case(name))
        })
        .or_else(|| fonts.first())
        .cloned()
}

/// Appends the font files below `dir` to `out`, as paths prefixed by `prefix`.
fn collect(dir: &Path, prefix: &str, depth: usize, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let rel = format!("{prefix}{name}");
        if path.is_dir() {
            if depth < MAX_DEPTH {
                collect(&path, &format!("{rel}/"), depth.saturating_add(1), out);
            }
        } else if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| EXTENSIONS.iter().any(|font| font.eq_ignore_ascii_case(ext)))
        {
            out.push(rel);
        }
    }
}
