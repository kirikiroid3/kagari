//! Where a debugger front end's runtime is: the headless query engine on a thread of this
//! process, a game launched in a window with `--debug-stdio`, or a running game's
//! `--debug-port`.

use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;

use crate::headless::StartOptions;
use crate::transport::Link;

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
                    .spawn(move || crate::headless::serve(&dir, &start, server))
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
