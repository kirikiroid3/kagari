//! The system font directories `system_font("…")` searches.

use std::path::PathBuf;

/// This machine's system font directories that exist, in search order, the user's before
/// the system's:
///
/// - Windows: `%LOCALAPPDATA%\Microsoft\Windows\Fonts`, `%WINDIR%\Fonts`;
/// - macOS: `~/Library/Fonts`, `/Library/Fonts`, `/System/Library/Fonts`;
/// - elsewhere: `$XDG_DATA_HOME/fonts` (default `~/.local/share/fonts`), `~/.fonts`, then
///   `fonts` in each of `$XDG_DATA_DIRS` (default `/usr/local/share:/usr/share`).
pub fn system_font_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for dir in candidates() {
        if dir.is_dir() && !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    dirs
}

/// The absolute path in the environment variable `key`; relative values are ignored, as the
/// XDG base directory specification asks.
fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

#[cfg(windows)]
fn candidates() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(local) = env_path("LOCALAPPDATA") {
        dirs.push(local.join("Microsoft").join("Windows").join("Fonts"));
    }
    let windir = env_path("WINDIR")
        .or_else(|| env_path("SystemRoot"))
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    dirs.push(windir.join("Fonts"));
    dirs
}

#[cfg(target_os = "macos")]
fn candidates() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = env_path("HOME") {
        dirs.push(home.join("Library").join("Fonts"));
    }
    dirs.push(PathBuf::from("/Library/Fonts"));
    dirs.push(PathBuf::from("/System/Library/Fonts"));
    dirs
}

#[cfg(not(any(windows, target_os = "macos")))]
fn candidates() -> Vec<PathBuf> {
    let home = env_path("HOME");
    let mut dirs = Vec::new();
    if let Some(data_home) =
        env_path("XDG_DATA_HOME").or_else(|| home.as_ref().map(|home| home.join(".local/share")))
    {
        dirs.push(data_home.join("fonts"));
    }
    if let Some(home) = &home {
        dirs.push(home.join(".fonts"));
    }
    let data_dirs = std::env::var_os("XDG_DATA_DIRS")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    for dir in std::env::split_paths(&data_dirs) {
        if dir.is_absolute() {
            dirs.push(dir.join("fonts"));
        }
    }
    dirs
}
