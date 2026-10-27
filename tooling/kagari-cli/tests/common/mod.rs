#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

/// Runs the `kagari` binary in `cwd`.
pub fn kagari(args: &[&str], cwd: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kagari"))
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run kagari")
}

/// A temp dir with a fresh project in `<tmp>/game`, named `game`.
pub fn init_project() -> TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let game = tmp.path().join("game");
    let output = kagari(
        &["init", game.to_str().unwrap(), "--name", "game"],
        tmp.path(),
    );
    assert!(output.status.success(), "{}", stderr(&output));
    tmp
}

/// The project directory of [`init_project`].
pub fn game(tmp: &TempDir) -> PathBuf {
    tmp.path().join("game").canonicalize().unwrap()
}

pub fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}
