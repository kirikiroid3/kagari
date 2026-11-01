//! The game directory, its bytecode, the boot, and thumbnail uploads.

use std::fs;
use std::path::{Path, PathBuf};

use kagari_engine::{BootError, Engine, Frame};
use kagari_render::Renderer;

use crate::exit::{Failure, USAGE};
use crate::platform::DesktopPlatform;

/// The bytecode file `kagari build` writes.
const PROGRAM: &str = "game.kagari";

/// `GAME_DIR`, or the executable's directory; it must hold `game.kagari`. On macOS, an executable
/// in an app bundle (`<name>.app/Contents/MacOS`) whose directory lacks `game.kagari` reads the
/// bundle's `Contents/Resources`, where `kagari release --macos` puts the game.
pub fn game_dir(arg: Option<PathBuf>) -> Result<PathBuf, Failure> {
    let dir = match arg {
        Some(dir) => dir,
        None => {
            let exe = std::env::current_exe()
                .map_err(|e| Failure::new(USAGE, format!("the executable's path: {e}")))?;
            let dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
            bundle_resources(&dir).unwrap_or(dir)
        }
    };
    if !dir.join(PROGRAM).is_file() {
        return Err(Failure::new(
            USAGE,
            format!(
                "`{}` does not contain `{PROGRAM}`; pass the directory written by `kagari build`, such as `target/v0`",
                dir.display()
            ),
        ));
    }
    Ok(dir)
}

/// The `Contents/Resources` of the app bundle whose `Contents/MacOS` is `exe_dir`, when `exe_dir`
/// has no `game.kagari` and that directory has one.
fn bundle_resources(exe_dir: &Path) -> Option<PathBuf> {
    if !cfg!(target_os = "macos") || exe_dir.join(PROGRAM).is_file() {
        return None;
    }
    let contents = exe_dir.parent()?;
    if exe_dir.file_name()? != "MacOS" || contents.file_name()? != "Contents" {
        return None;
    }
    let resources = contents.join("Resources");
    resources.join(PROGRAM).is_file().then_some(resources)
}

/// The whole `game.kagari`.
pub fn read_program(dir: &Path) -> Result<Vec<u8>, Failure> {
    let path = dir.join(PROGRAM);
    fs::read(&path).map_err(|e| Failure::new(USAGE, format!("{}: {e}", path.display())))
}

/// Boots at time 0. A trap exits 3, every other boot error 2.
pub fn boot(bytes: &[u8], platform: &mut DesktopPlatform) -> Result<Engine, Failure> {
    Engine::boot(bytes, platform, 0).map_err(|e| match e {
        BootError::Trap(message) => Failure::stopped(&message),
        other => Failure::new(USAGE, format!("cannot start the game: {other}")),
    })
}

/// Uploads every save thumbnail `frame` draws whose generation the renderer lacks.
pub fn upload_thumbnails(renderer: &mut Renderer, engine: &Engine, frame: &Frame) {
    for (slot, _) in renderer.stale_thumbnails(frame) {
        if let Some((generation, rgba, width, height)) = engine.thumbnail(slot) {
            renderer.set_thumbnail(slot, generation, width, height, &rgba);
        }
    }
}
