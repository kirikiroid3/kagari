//! A debug adapter (feature `dap`): the Debug Adapter Protocol over stdin/stdout. The editor
//! speaks DAP with source lines; the runtime speaks the Kagari debug protocol (KDP) with step
//! IDs and byte offsets, over the same framing. One loop serves both connections: editor
//! requests become runtime requests (a pending map says what to do with each answer), runtime
//! events become editor events. Nothing but DAP is written to stdout.
//!
//! A front end runs it with a [`Launcher`], which knows projects: `launch.rs` reads the
//! editor's configuration into a [`launch::Prepared`] runtime, which this module connects.
//! `editor.rs` serves the editor's requests, `answers.rs` the runtime's answers and events,
//! `breakpoints.rs` keeps the breakpoints of every file, `frames.rs` converts stacks and
//! variables, and `stepping.rs` repeats code steps until the line changes.

mod answers;
mod breakpoints;
mod editor;
mod frames;
mod launch;
mod stepping;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

use crate::client::{Attached, Locator};
use crate::protocol::{Message, StoppedBody, command};
use crate::transport::{Connection, Link};
use answers::Waiting;
use breakpoints::Breakpoints;
use frames::{Lines, VarRefs};
use stepping::{CodeAt, Stepping};

pub use launch::{Launcher, Opened, Place};

/// The one thread a story has.
const THREAD: i64 = 1;

/// Serves an editor on stdin/stdout until it disconnects. `launcher` opens the projects that
/// `launch` and `attach` name.
pub fn run_stdio(launcher: &mut dyn Launcher) {
    let (done, flushed) = mpsc::channel();
    let editor = Connection::new(Link::over(
        io::stdin(),
        Stdout {
            out: io::stdout(),
            done,
        },
    ));
    Adapter::new(editor, launcher).run();
    // Let the writer thread write the last answers before the process ends.
    let _ = flushed.recv_timeout(Duration::from_secs(2));
}

/// Standard output for the editor link; says when its writer thread let go of it.
struct Stdout {
    out: io::Stdout,
    done: mpsc::Sender<()>,
}

impl Write for Stdout {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.out.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

impl Drop for Stdout {
    fn drop(&mut self) {
        let _ = self.out.flush();
        let _ = self.done.send(());
    }
}

/// A connected runtime.
struct Session {
    runtime: Connection,
    /// Kept alive: dropping it ends a launched game.
    attached: Attached,
    locator: Locator,
}

struct Adapter<'l> {
    editor: Connection,
    launcher: &'l mut dyn Launcher,
    session: Option<Session>,
    pending: HashMap<i64, Waiting>,
    /// Runtime events held until the `launch`/`attach` is answered.
    held: Option<Vec<Message>>,
    lines: Lines,
    breakpoints: Breakpoints,
    refs: VarRefs,
    /// The editor was told the story is stopped.
    told_stopped: bool,
    last_stop: Option<StoppedBody>,
    code_at: Option<CodeAt>,
    stepping: Option<Stepping>,
    done: bool,
}

impl<'l> Adapter<'l> {
    fn new(editor: Connection, launcher: &'l mut dyn Launcher) -> Adapter<'l> {
        Adapter {
            editor,
            launcher,
            session: None,
            pending: HashMap::new(),
            held: None,
            lines: Lines::default(),
            breakpoints: Breakpoints::default(),
            refs: VarRefs::default(),
            told_stopped: false,
            last_stop: None,
            code_at: None,
            stepping: None,
            done: false,
        }
    }

    fn run(&mut self) {
        while !self.done {
            while !self.done
                && let Some(message) = self.session.as_ref().and_then(|s| s.runtime.try_recv())
            {
                self.runtime_message(message);
            }
            if self.done {
                break;
            }
            if self.session.as_ref().is_some_and(|s| s.runtime.closed()) {
                self.runtime_gone();
                break;
            }
            match self.editor.recv_timeout(Duration::from_millis(5)) {
                Some(message) => self.editor_message(message),
                None if self.editor.closed() => {
                    self.end_session();
                    break;
                }
                None => {}
            }
        }
    }

    fn output(&self, text: impl Into<String>) {
        output(&self.editor, text);
    }

    /// Sends a runtime request; `waiting` says what to do with its answer.
    fn forward(&mut self, command: &str, arguments: Value, waiting: Waiting) -> Result<(), String> {
        let session = self
            .session
            .as_ref()
            .ok_or("no game is being debugged: launch or attach first")?;
        let seq = session.runtime.request(command, arguments);
        self.pending.insert(seq, waiting);
        Ok(())
    }

    /// Ends the runtime side: KDP `disconnect` (waiting briefly for its answer), then a
    /// launched game is ended.
    fn end_session(&mut self) {
        let Some(mut session) = self.session.take() else {
            return;
        };
        let bye = session.runtime.request(command::DISCONNECT, Value::Null);
        for _ in 0..100 {
            match session.runtime.recv_timeout(Duration::from_millis(20)) {
                Some(Message::Response { request_seq, .. }) if request_seq == bye => break,
                None if session.runtime.closed() => break,
                _ => {}
            }
        }
        session.attached.kill();
        self.fail_pending("the debug session ended");
    }

    /// The runtime closed its link: the session is over.
    fn runtime_gone(&mut self) {
        for message in self.held.take().unwrap_or_default() {
            self.runtime_message(message);
        }
        if let Some(mut session) = self.session.take() {
            session.attached.kill();
        }
        self.fail_pending("the game is gone");
        self.editor.event("terminated", json!({}));
        self.done = true;
    }

    /// Answers every editor request still waiting for the runtime with an error.
    fn fail_pending(&mut self, message: &str) {
        let mut waiting: Vec<(i64, Waiting)> = self.pending.drain().collect();
        waiting.sort_by_key(|(seq, _)| *seq);
        for (_, waiting) in waiting {
            if let Some((seq, command)) = waiting.editor_request() {
                self.editor.respond(seq, command, Err(message.to_owned()));
            }
        }
    }
}

/// A console line for the editor.
fn output(editor: &Connection, text: impl Into<String>) {
    editor.event(
        "output",
        json!({ "category": "console", "output": text.into() }),
    );
}
