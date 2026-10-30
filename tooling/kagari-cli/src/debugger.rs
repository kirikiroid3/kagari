//! What the debugger front ends (`kagari debug`, `kagari dap`) need of the project: its target
//! directory and source root, the debug build, and where the runtime is. The front ends
//! themselves are in `kagari-debugger` (features `client`, `dap`); [`Projects`] is the
//! adapter's [`Launcher`].

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;

use kagari_debugger::dap::{Launcher, Opened, Place};
use kagari_project::{MANIFEST_FILE, Project, Target};

use crate::workspace;

/// Opens projects for the debug adapter.
pub struct Projects;

impl Launcher for Projects {
    fn open(
        &mut self,
        place: &Place,
        build: bool,
        out: &mut dyn FnMut(&str),
    ) -> Result<Opened, String> {
        let manifest = manifest_path(place.cwd.as_deref(), place.manifest_path.as_deref())?;
        open(manifest.as_deref(), place.target.as_deref(), build, out)
    }

    fn default_runtime(&self) -> PathBuf {
        default_runtime()
    }
}

/// The project at `manifest_path` (or the nearest one) and its `target` directory (or the
/// first); with `build`, built in the debug profile first, each output line to `out`.
pub fn open(
    manifest_path: Option<&Path>,
    target: Option<&str>,
    build: bool,
    out: &mut dyn FnMut(&str),
) -> Result<Opened, String> {
    let project = workspace::load(manifest_path).map_err(|f| f.0)?;
    let target = match target {
        Some(name) => Target::parse(name)
            .filter(|t| project.manifest.targets.contains(t))
            .ok_or_else(|| format!("target `{name}` is not listed in `build.targets`"))?,
        None => *project
            .manifest
            .targets
            .first()
            .ok_or("the project lists no build target")?,
    };
    if build {
        build_debug(&project, target, out)?;
    }
    // Source file names in the bytecode are relative to the root file's directory.
    let entry = project.entry_path();
    let source_root = entry
        .parent()
        .map_or_else(|| project.root.clone(), Path::to_path_buf);
    Ok(Opened {
        game_dir: project.out_dir(target),
        source_root,
        outputs: vec![project.build_dir()],
        project_root: project.root,
    })
}

/// The `kagari.toml` to load: `manifest_path` (relative to `cwd`), or the nearest one from
/// `cwd`, or (`None`) the nearest one from the current directory.
fn manifest_path(
    cwd: Option<&Path>,
    manifest_path: Option<&Path>,
) -> Result<Option<PathBuf>, String> {
    let path = match (cwd, manifest_path) {
        (Some(cwd), Some(path)) => cwd.join(path),
        (None, Some(path)) => path.to_path_buf(),
        (Some(cwd), None) => cwd
            .ancestors()
            .map(|dir| dir.join(MANIFEST_FILE))
            .find(|candidate| candidate.is_file())
            .ok_or_else(|| {
                format!(
                    "no `{MANIFEST_FILE}` in {} or a parent directory",
                    cwd.display()
                )
            })?,
        (None, None) => return Ok(None),
    };
    Ok(Some(if path.is_dir() {
        path.join(MANIFEST_FILE)
    } else {
        path
    }))
}

/// Runs `kagari build --profile debug` for the project and target as a child process (its
/// stdout may be the adapter's protocol); each output line goes to `out`.
fn build_debug(project: &Project, target: Target, out: &mut dyn FnMut(&str)) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot find `kagari`: {e}"))?;
    let mut child = Command::new(exe)
        .args(["build", "--profile", "debug", "--target", target.as_str()])
        .arg("--manifest-path")
        .arg(&project.manifest_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run `kagari build`: {e}"))?;
    let stderr = child.stderr.take();
    let errors = thread::spawn(move || {
        stderr
            .map(|e| BufReader::new(e).lines().map_while(Result::ok).collect())
            .unwrap_or_else(Vec::<String>::new)
    });
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            out(&line);
        }
    }
    for line in errors.join().unwrap_or_default() {
        out(&line);
    }
    let status = child
        .wait()
        .map_err(|e| format!("`kagari build` failed: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("`kagari build --profile debug` failed".to_owned())
    }
}

/// `kagari-desktop-runtime`: next to this executable, else on `PATH`.
pub fn default_runtime() -> PathBuf {
    let name = format!("kagari-desktop-runtime{}", std::env::consts::EXE_SUFFIX);
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(&name)))
        .filter(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from(name))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn the_manifest_is_found_from_cwd() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(MANIFEST_FILE), "").unwrap();
        let nested = dir.path().join("src");
        std::fs::create_dir(&nested).unwrap();
        assert_eq!(
            manifest_path(Some(&nested), None).unwrap(),
            Some(dir.path().join(MANIFEST_FILE))
        );
        assert_eq!(
            manifest_path(Some(dir.path()), Some(Path::new("kagari.toml"))).unwrap(),
            Some(dir.path().join(MANIFEST_FILE))
        );
        assert_eq!(manifest_path(None, None).unwrap(), None);
    }
}
