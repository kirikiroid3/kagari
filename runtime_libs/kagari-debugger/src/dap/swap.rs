//! Scene hot swap from the editor: `kagari/swap` (or `evaluate` of `swap`, or a change a
//! `watch` session sees) rebuilds the debug profile through the [`Launcher`](super::Launcher),
//! then asks the runtime to swap the new game in. When it has, the new sources take over: the
//! frames and variable references are stale, and every line breakpoint is resolved again (the
//! runtime dropped its instruction breakpoints).

use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::Adapter;
use super::answers::Waiting;
use crate::client::Locator;
use crate::protocol::{SwapBody, command};

/// How often a `watch` session looks at the project's files.
pub(super) const WATCH_INTERVAL: Duration = Duration::from_millis(500);

/// Who waits for a swap's outcome.
pub(super) enum SwapFor {
    /// The editor's `kagari/swap` (its `seq`): the runtime's body.
    Request(i64),
    /// The editor's `evaluate` of `swap` (its `seq`): the summary.
    Evaluate(i64),
    /// A change a `watch` session saw: nobody.
    Watch,
}

impl SwapFor {
    /// The editor request it answers.
    pub fn editor_request(&self) -> Option<(i64, &str)> {
        match self {
            SwapFor::Request(seq) => Some((*seq, command::SWAP)),
            SwapFor::Evaluate(seq) => Some((*seq, "evaluate")),
            SwapFor::Watch => None,
        }
    }
}

impl Adapter<'_> {
    /// Rebuilds the debug profile (its output to the console) and asks the runtime to swap the
    /// new game in. A failed build is the error.
    pub(super) fn swap(&mut self, swap_for: SwapFor) -> Result<(), String> {
        let place = self
            .session
            .as_ref()
            .ok_or("no game is being debugged: launch or attach first")?
            .place
            .clone();
        let editor = &self.editor;
        let opened = self.launcher.open(&place, true, &mut |line| {
            super::output(editor, format!("{line}\n"));
        })?;
        // The build has seen the files as they are.
        if let Some(watcher) = self.session.as_mut().and_then(|s| s.watcher.as_mut()) {
            watcher.settle();
        }
        let waiting = Waiting::Swap {
            swap_for,
            locator: Box::new(opened.locator()),
        };
        self.forward(command::SWAP, Value::Null, waiting)
    }

    /// The runtime's answer to `kagari/swap`; `locator` reads the rebuilt game's sources.
    pub(super) fn swapped(
        &mut self,
        swap_for: SwapFor,
        locator: Locator,
        result: Result<Value, String>,
    ) {
        let body = match result {
            Ok(body) => body,
            Err(message) => {
                match swap_for.editor_request() {
                    Some((seq, command)) => self.editor.respond(seq, command, Err(message)),
                    None => self.output(format!("swap: {message}\n")),
                }
                return;
            }
        };
        let summary = serde_json::from_value::<SwapBody>(body.clone())
            .unwrap_or_default()
            .summary();
        if let Some(session) = self.session.as_mut() {
            session.locator = locator;
        }
        self.refs.clear();
        self.code_at = None;
        self.stepping = None;
        self.output(format!("{summary}\n"));
        match swap_for {
            SwapFor::Request(seq) => self.editor.respond(seq, command::SWAP, Ok(body)),
            SwapFor::Evaluate(seq) => self.editor.respond(
                seq,
                "evaluate",
                Ok(json!({ "result": summary, "variablesReference": 0 })),
            ),
            SwapFor::Watch => {}
        }
        if let Err(message) = self.reset_breakpoints() {
            self.output(format!("breakpoints: {message}\n"));
        }
    }

    /// `watch`: at most every [`WATCH_INTERVAL`], looks at the project's files; a change
    /// rebuilds and swaps as `kagari/swap` does.
    pub(super) fn watch(&mut self) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let Some(watcher) = session.watcher.as_mut() else {
            return;
        };
        if session.polled.elapsed() < WATCH_INTERVAL {
            return;
        }
        session.polled = Instant::now();
        if !watcher.changed() {
            return;
        }
        self.output("source changed: rebuilding\n");
        if let Err(message) = self.swap(SwapFor::Watch) {
            self.output(format!("{message}\n"));
        }
    }
}
