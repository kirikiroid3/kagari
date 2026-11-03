//! The REPL's terminal: command lines in, answers out. On a terminal the lines are read with a
//! line editor (rustyline): ←/→ move in the line, ↑/↓ recall the earlier lines that start with
//! what was typed. Elsewhere (a pipe, a file) the lines are read as they come.
//!
//! The editor reads on a thread of its own, one line per [`Console::prompt`], so the terminal is
//! in raw mode only while the prompt is up. Output goes through rustyline's external printer,
//! which redraws the prompt and the line being typed below it, on a thread of its own as well:
//! the printer can block (its queue holds one message, which only a prompt takes), the REPL never.

use std::io::{self, BufRead, IsTerminal, Write};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use rustyline::error::ReadlineError;
use rustyline::{
    Cmd, ConditionalEventHandler, DefaultEditor, Event, EventContext, EventHandler,
    ExternalPrinter, KeyCode, KeyEvent, Modifiers, Movement, RepeatCount,
};

const PROMPT: &str = "(kagari) ";

/// How long [`Console::close`] waits for the printer thread to print what is left.
const FLUSH_TIMEOUT: Duration = Duration::from_millis(500);

pub struct Console {
    lines: Receiver<String>,
    /// `None` when standard input or output is not a terminal.
    editor: Option<Editor>,
    /// The prompt is up: a line is being waited for.
    prompted: bool,
}

/// The main thread's end of the editor and printer threads.
struct Editor {
    /// Asks for one line.
    ask: Sender<()>,
    /// To the printer thread.
    out: Sender<String>,
    /// Disconnects when the printer thread has printed everything.
    printed: Receiver<()>,
}

impl Console {
    pub fn start() -> Console {
        let (send, lines) = mpsc::channel();
        let terminal = io::stdin().is_terminal() && io::stdout().is_terminal();
        let editor = if terminal { start_editor(&send) } else { None };
        if editor.is_none() {
            thread::spawn(move || {
                for line in io::stdin().lock().lines() {
                    let Ok(line) = line else { break };
                    if send.send(line).is_err() {
                        break;
                    }
                }
            });
        }
        Console {
            lines,
            editor,
            prompted: false,
        }
    }

    /// Puts the prompt up, unless it is.
    pub fn prompt(&mut self) {
        if self.prompted {
            return;
        }
        self.prompted = true;
        match &self.editor {
            Some(editor) => {
                let _ = editor.ask.send(());
            }
            None => {
                print!("{PROMPT}");
                let _ = io::stdout().flush();
            }
        }
    }

    /// The next line, if one comes within `timeout`; `Disconnected` at the end of the input.
    pub fn line(&mut self, timeout: Duration) -> Result<String, RecvTimeoutError> {
        let line = self.lines.recv_timeout(timeout);
        if line.is_ok() {
            self.prompted = false;
        }
        line
    }

    /// Prints `text` and a line break.
    pub fn println(&mut self, text: &str) {
        self.print(format!("{text}\n"));
    }

    /// Prints `text`. Without the editor, a prompt that was up is put up again after it.
    pub fn print(&mut self, text: String) {
        match &mut self.editor {
            Some(editor) => {
                let _ = editor.out.send(text);
            }
            None => {
                print!("{text}");
                let _ = io::stdout().flush();
                self.prompted = false;
            }
        }
    }

    /// Lets go of the terminal before the REPL ends. The editor's raw mode ends only with the
    /// line, so a prompt that is up waits for Enter (or Ctrl-C, Ctrl-D); then what is left to
    /// print is printed.
    pub fn close(&mut self) {
        if self.editor.is_some() && self.prompted {
            self.println("press Enter to exit");
            let _ = self.lines.recv();
            self.prompted = false;
        }
        if let Some(Editor { ask, out, printed }) = self.editor.take() {
            drop((ask, out));
            // At once, unless a message went to a prompt that closed before taking it: that
            // one blocks the printer until a prompt that will not come.
            let _ = printed.recv_timeout(FLUSH_TIMEOUT);
        }
    }
}

/// The editor and its thread; `None` if the terminal cannot have one.
fn start_editor(lines: &Sender<String>) -> Option<Editor> {
    let mut editor = DefaultEditor::new().ok()?;
    let mut printer = editor.create_external_printer().ok()?;
    let recall = Arc::new(Mutex::new(Recall::default()));
    for (key, older) in [(KeyCode::Up, true), (KeyCode::Down, false)] {
        let arrow = Arrow {
            recall: Arc::clone(&recall),
            older,
        };
        editor.bind_sequence(
            KeyEvent(key, Modifiers::NONE),
            EventHandler::Conditional(Box::new(arrow)),
        );
    }
    let (ask, asked) = mpsc::channel();
    let lines = lines.clone();
    thread::spawn(move || read(editor, &recall, &asked, &lines));
    let (out, queue) = mpsc::channel::<String>();
    let (done, printed) = mpsc::channel::<()>();
    thread::spawn(move || {
        let _done = done;
        for text in queue {
            if let Err(error) = printer.print(text) {
                eprintln!("error: cannot print: {error}");
            }
        }
    });
    Some(Editor { ask, out, printed })
}

/// The editor thread: a line for each ask, until Ctrl-D or the console is gone.
fn read(
    mut editor: DefaultEditor,
    recall: &Mutex<Recall>,
    asked: &Receiver<()>,
    lines: &Sender<String>,
) {
    while asked.recv().is_ok() {
        if let Ok(mut recall) = recall.lock() {
            recall.search = None;
        }
        let line = match editor.readline(PROMPT) {
            Ok(line) => line,
            // Ctrl-C drops the line, as in a shell.
            Err(ReadlineError::Interrupted) => String::new(),
            // Ctrl-D, or the terminal is gone: the end of the input.
            Err(_) => return,
        };
        if !line.trim().is_empty() {
            let _ = editor.add_history_entry(line.as_str());
            if let Ok(mut recall) = recall.lock() {
                recall.add(&line);
            }
        }
        if lines.send(line).is_err() {
            return;
        }
    }
}

/// ↑ (`older`) or ↓, bound to [`Recall`].
struct Arrow {
    recall: Arc<Mutex<Recall>>,
    older: bool,
}

impl ConditionalEventHandler for Arrow {
    fn handle(&self, _: &Event, _: RepeatCount, _: bool, ctx: &EventContext<'_>) -> Option<Cmd> {
        let mut recall = self.recall.lock().ok()?;
        let line = if self.older {
            recall.older(ctx.line())
        } else {
            recall.newer(ctx.line())
        };
        Some(line.map_or(Cmd::Noop, |line| {
            Cmd::Replace(Movement::WholeLine, Some(line))
        }))
    }
}

/// History search by prefix, as zsh's `history-beginning-search`: ↑ shows the newest earlier line
/// that starts with the typed text, then the one before it; ↓ goes back, and past the newest
/// match restores the typed text. Editing a shown line starts a new search from it.
#[derive(Default)]
struct Recall {
    /// The lines entered, oldest first, each once: a repeated line moves to the end.
    lines: Vec<String>,
    search: Option<Search>,
}

struct Search {
    /// What was typed when the search began.
    typed: String,
    /// The index in `lines` of the line shown.
    at: usize,
}

impl Recall {
    fn add(&mut self, line: &str) {
        self.lines.retain(|l| l != line);
        self.lines.push(line.to_owned());
        self.search = None;
    }

    /// The search going on in `line`: none if it was edited since a line was shown.
    fn current(&mut self, line: &str) -> Option<Search> {
        self.search
            .take()
            .filter(|search| self.lines.get(search.at).is_some_and(|shown| shown == line))
    }

    /// ↑: the line to show, if there is an older match.
    fn older(&mut self, line: &str) -> Option<String> {
        let (typed, end) = match self.current(line) {
            Some(search) => (search.typed, search.at),
            None => (line.to_owned(), self.lines.len()),
        };
        let earlier = self.lines.get(..end)?;
        match earlier
            .iter()
            .rposition(|l| l.starts_with(&typed) && l != line)
        {
            Some(at) => {
                let shown = earlier.get(at).cloned();
                self.search = Some(Search { typed, at });
                shown
            }
            None => {
                // Nothing older: stay on the line shown.
                if end < self.lines.len() {
                    self.search = Some(Search { typed, at: end });
                }
                None
            }
        }
    }

    /// ↓: the line to show, if a search is going on.
    fn newer(&mut self, line: &str) -> Option<String> {
        let search = self.current(line)?;
        let start = search.at.checked_add(1)?;
        let later = self.lines.get(start..)?;
        match later.iter().position(|l| l.starts_with(&search.typed)) {
            Some(offset) => {
                let shown = later.get(offset).cloned();
                self.search = Some(Search {
                    typed: search.typed,
                    at: start.checked_add(offset)?,
                });
                shown
            }
            None => Some(search.typed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Recall;

    fn recall(lines: &[&str]) -> Recall {
        let mut recall = Recall::default();
        for line in lines {
            recall.add(line);
        }
        recall
    }

    /// Presses ↑ (`true`) and ↓ in turn from `typed`; the line after each key.
    fn keys(recall: &mut Recall, typed: &str, keys: &[bool]) -> Vec<String> {
        let mut line = typed.to_owned();
        keys.iter()
            .map(|&older| {
                let shown = if older {
                    recall.older(&line)
                } else {
                    recall.newer(&line)
                };
                if let Some(shown) = shown {
                    line = shown;
                }
                line.clone()
            })
            .collect()
    }

    #[test]
    fn up_shows_the_matches_newest_first_and_down_restores_the_typed_text() {
        let mut r = recall(&["break a.kis:3", "next", "break b.kis:9", "print x"]);
        assert_eq!(
            keys(&mut r, "br", &[true, true, true, false, false, false]),
            [
                "break b.kis:9",
                "break a.kis:3",
                "break a.kis:3",
                "break b.kis:9",
                "br",
                "br",
            ]
        );
    }

    #[test]
    fn an_empty_line_walks_the_whole_history() {
        let mut r = recall(&["next", "step"]);
        assert_eq!(
            keys(&mut r, "", &[true, true, false]),
            ["step", "next", "step"]
        );
    }

    #[test]
    fn a_repeated_line_is_recalled_once_at_its_latest_place() {
        let mut r = recall(&["next", "step", "next"]);
        assert_eq!(
            keys(&mut r, "", &[true, true, true]),
            ["next", "step", "step"]
        );
    }

    #[test]
    fn the_line_as_typed_is_skipped() {
        let mut r = recall(&["next 2", "next"]);
        assert_eq!(keys(&mut r, "next", &[true]), ["next 2"]);
    }

    #[test]
    fn editing_a_shown_line_starts_a_new_search_from_it() {
        let mut r = recall(&["break a.kis:3", "break b.kis:9", "next"]);
        assert_eq!(keys(&mut r, "b", &[true]), ["break b.kis:9"]);
        assert_eq!(
            keys(&mut r, "break a", &[true, false]),
            ["break a.kis:3", "break a"]
        );
    }
}
