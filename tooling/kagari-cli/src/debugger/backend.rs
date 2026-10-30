//! Where a debugger front end's runtime is: the headless query engine on a thread of this
//! process, a game launched in a window with `--debug-stdio`, or a running game's
//! `--debug-port`.

use std::io::{BufRead, BufReader};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;

use kagari_debugger::Link;
use kagari_debugger::headless::StartOptions;
use kagari_project::{Project, Target};

use crate::workspace;

/// A runtime to debug.
pub enum Backend {
    /// The query engine on the game in `dir`.
    Headless { dir: PathBuf, start: StartOptions },
    /// `runtime dir --debug-stdio args…`.
    Window {
        runtime: PathBuf,
        dir: PathBuf,
        args: Vec<String>,
    },
    /// A game listening on `host:port`.
    Attach { address: String },
}

/// A connected runtime; dropping it ends a launched game.
pub struct Attached {
    pub link: Option<Link>,
    child: Option<Child>,
}

impl Attached {
    /// Ends a launched game.
    pub fn kill(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.child = None;
    }
}

impl Drop for Attached {
    fn drop(&mut self) {
        self.kill();
    }
}

impl Backend {
    pub fn connect(self) -> Result<Attached, String> {
        match self {
            Backend::Headless { dir, start } => {
                let (client, server) = Link::pair();
                thread::Builder::new()
                    .name("kagari-query-engine".to_owned())
                    .spawn(move || kagari_debugger::headless::serve(&dir, &start, server))
                    .map_err(|e| format!("cannot start the query engine: {e}"))?;
                Ok(Attached {
                    link: Some(client),
                    child: None,
                })
            }
            Backend::Window { runtime, dir, args } => {
                let mut child = Command::new(&runtime)
                    .arg(&dir)
                    .arg("--debug-stdio")
                    .args(&args)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .spawn()
                    .map_err(|e| format!("cannot start `{}`: {e}", runtime.display()))?;
                let (Some(stdout), Some(stdin)) = (child.stdout.take(), child.stdin.take()) else {
                    return Err("the runtime has no standard input or output".to_owned());
                };
                Ok(Attached {
                    link: Some(Link::over(stdout, stdin)),
                    child: Some(child),
                })
            }
            Backend::Attach { address } => {
                let stream = TcpStream::connect(&address)
                    .map_err(|e| format!("cannot connect to {address}: {e}"))?;
                let link = Link::tcp(stream).map_err(|e| format!("{address}: {e}"))?;
                Ok(Attached {
                    link: Some(link),
                    child: None,
                })
            }
        }
    }
}

/// The project and the target directory to debug.
pub fn project(
    manifest_path: Option<&Path>,
    target: Option<&str>,
) -> Result<(Project, PathBuf), String> {
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
    let dir = project.out_dir(target);
    Ok((project, dir))
}

/// The directory the bytecode's source file names are relative to: the root file's.
pub fn source_root(project: &Project) -> PathBuf {
    let entry = project.entry_path();
    entry
        .parent()
        .map_or_else(|| project.root.clone(), Path::to_path_buf)
}

/// Runs `kagari build --profile debug` for the project and target; each output line goes to
/// `out`.
pub fn build_debug(project: &Project, dir: &Path, out: &mut dyn FnMut(&str)) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot find `kagari`: {e}"))?;
    let target = dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("a malformed target directory")?;
    let mut child = Command::new(exe)
        .args([
            "build",
            "--profile",
            "debug",
            "--target",
            target,
            "--manifest-path",
        ])
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
