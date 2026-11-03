//! Where a debugger front end's runtime is: the headless query engine on a thread of this
//! process, a game launched in a window with `--debug-stdio`, or a running game's
//! `--debug-port`.

use std::io::{BufRead, BufReader, Read};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

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

/// How long [`Attached::rest_of_log`] waits for a game that went away to close its log.
const LAST_LOG_TIMEOUT: Duration = Duration::from_millis(500);

/// A connected runtime; dropping it ends a launched game.
pub struct Attached {
    pub link: Option<Link>,
    child: Option<Child>,
    /// A launched game's standard error (its log), line by line. A thread reads it all the
    /// time, so the game never blocks on a full pipe.
    log: Option<Receiver<String>>,
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

    /// The next line a launched game wrote to its standard error, if one is waiting.
    pub fn log_line(&self) -> Option<String> {
        self.log.as_ref()?.try_recv().ok()
    }

    /// The lines a launched game still writes to its standard error until it closes it (it
    /// exits), waiting at most [`LAST_LOG_TIMEOUT`] in all: what a game that went away said last.
    pub fn rest_of_log(&self) -> Vec<String> {
        let mut lines = Vec::new();
        let Some(log) = self.log.as_ref() else {
            return lines;
        };
        let start = Instant::now();
        while let Some(left) = LAST_LOG_TIMEOUT.checked_sub(start.elapsed()) {
            match log.recv_timeout(left) {
                Ok(line) => lines.push(line),
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
            }
        }
        lines
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
                    log: None,
                })
            }
            Backend::Window { runtime, dir, args } => {
                let mut child = Command::new(&runtime)
                    .arg(&dir)
                    .arg("--debug-stdio")
                    .args(&args)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .map_err(|e| format!("cannot start `{}`: {e}", runtime.display()))?;
                let (Some(stdout), Some(stdin), Some(stderr)) =
                    (child.stdout.take(), child.stdin.take(), child.stderr.take())
                else {
                    let _ = child.kill();
                    return Err("the runtime has no standard input or output".to_owned());
                };
                let (send, log) = mpsc::channel();
                let reader = thread::Builder::new()
                    .name("kagari-runtime-log".to_owned())
                    .spawn(move || read_log(stderr, &send));
                if let Err(e) = reader {
                    let _ = child.kill();
                    return Err(format!("cannot read the runtime's log: {e}"));
                }
                Ok(Attached {
                    link: Some(Link::over(stdout, stdin)),
                    child: Some(child),
                    log: Some(log),
                })
            }
            Backend::Attach { address } => {
                let stream = TcpStream::connect(&address)
                    .map_err(|e| format!("cannot connect to {address}: {e}"))?;
                let link = Link::tcp(stream).map_err(|e| format!("{address}: {e}"))?;
                Ok(Attached {
                    link: Some(link),
                    child: None,
                    log: None,
                })
            }
        }
    }
}

/// Sends each line of `stderr` (invalid UTF-8 replaced, the line break dropped) until it ends.
fn read_log(stderr: impl Read, lines: &Sender<String>) {
    let mut stderr = BufReader::new(stderr);
    let mut line = Vec::new();
    loop {
        line.clear();
        match stderr.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        let text = String::from_utf8_lossy(&line);
        let text = text.trim_end_matches(['\n', '\r']);
        // Nobody listens any more: keep reading, so the game does not block.
        let _ = lines.send(text.to_owned());
    }
}

#[cfg(all(test, unix))]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::protocol::Message;

    /// `/bin/sh` as the runtime runs the "game directory" as its script.
    fn launch(script: &str) -> (tempfile::TempDir, Attached) {
        let tmp = tempfile::tempdir().unwrap();
        let game = tmp.path().join("game.sh");
        std::fs::write(&game, script).unwrap();
        let backend = Backend::Window {
            runtime: PathBuf::from("/bin/sh"),
            dir: game,
            args: Vec::new(),
        };
        (tmp, backend.connect().unwrap())
    }

    #[test]
    fn a_launched_game_s_standard_error_comes_line_by_line_to_its_end() {
        let (_tmp, attached) =
            launch(r"printf 'one\n' >&2; printf 'bad \377\r\n' >&2; printf 'last' >&2");
        assert_eq!(attached.rest_of_log(), ["one", "bad \u{FFFD}", "last"]);
        assert_eq!(attached.log_line(), None);
    }

    #[test]
    fn a_game_that_logs_more_than_a_pipe_holds_is_not_blocked() {
        // About 1 MB of log, far more than a pipe holds, then an event on the debug channel.
        let frame = r#"{"seq":1,"type":"event","event":"done","body":null}"#;
        let (_tmp, mut attached) = launch(&format!(
            "i=0; while [ $i -lt 100000 ]; do echo 'a log line' >&2; i=$((i+1)); done\n\
             printf 'Content-Length: {}\\r\\n\\r\\n%s' '{frame}'\n",
            frame.len()
        ));
        let link = attached.link.take().unwrap();
        let message = link.incoming.recv_timeout(Duration::from_secs(60));
        assert!(
            matches!(&message, Ok(Message::Event { event, .. }) if event == "done"),
            "{message:?}"
        );
    }
}
