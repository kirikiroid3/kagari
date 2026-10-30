//! The runtime's answers and events: each answer finishes what its [`Waiting`] says; events
//! become the editor's.

use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::swap::SwapFor;
use super::{Adapter, THREAD};
use crate::client::{Locator, text};
use crate::protocol::{
    BreakpointsBody, InitializeBody, Message, OutputBody, StoppedBody, command, event,
};

/// What to do with a runtime's answer.
pub(super) enum Waiting {
    /// KDP `initialize` of a `launch`/`attach` (the editor's request).
    Started {
        seq: i64,
        command: String,
    },
    /// A breakpoint update; already answered to the editor.
    Breakpoints,
    /// Nobody waits; errors go to the console.
    Silent,
    StackTrace {
        seq: i64,
        start: usize,
        levels: Option<usize>,
    },
    Scopes {
        seq: i64,
    },
    /// `continue`, `next`, `stepIn`, `stepOut`, `pause`.
    Resume {
        seq: i64,
        command: String,
    },
    /// `evaluate` of a command line.
    Evaluate {
        seq: i64,
        command: &'static str,
    },
    /// A `kagari/…` request: the body unchanged.
    Custom {
        seq: i64,
        command: String,
    },
    /// The stack of a stop inside code: where a line step starts.
    CodeStop,
    /// The stack after a code step: still on the line it started from?
    StepCheck(StoppedBody),
    /// A code step sent again; the stop to show if it fails.
    Restep(StoppedBody),
    /// `kagari/swap`; `locator` reads the rebuilt game's sources.
    Swap {
        swap_for: SwapFor,
        locator: Box<Locator>,
    },
}

impl Waiting {
    /// The editor request it answers.
    pub fn editor_request(&self) -> Option<(i64, &str)> {
        match self {
            Waiting::Started { seq, command }
            | Waiting::Resume { seq, command }
            | Waiting::Custom { seq, command } => Some((*seq, command)),
            Waiting::StackTrace { seq, .. } => Some((*seq, "stackTrace")),
            Waiting::Scopes { seq } => Some((*seq, "scopes")),
            Waiting::Evaluate { seq, .. } => Some((*seq, "evaluate")),
            Waiting::Swap { swap_for, .. } => swap_for.editor_request(),
            Waiting::Breakpoints
            | Waiting::Silent
            | Waiting::CodeStop
            | Waiting::StepCheck(_)
            | Waiting::Restep(_) => None,
        }
    }
}

/// A body as `T`; a malformed one is its default.
fn parse<T: DeserializeOwned + Default>(body: Value) -> T {
    serde_json::from_value(body).unwrap_or_default()
}

impl Adapter<'_> {
    pub(super) fn runtime_message(&mut self, message: Message) {
        if let Some(held) = &mut self.held
            && matches!(message, Message::Event { .. })
        {
            held.push(message);
            return;
        }
        match message {
            Message::Response {
                request_seq,
                success,
                message,
                body,
                ..
            } => {
                if let Some(waiting) = self.pending.remove(&request_seq) {
                    let result = match success {
                        true => Ok(body),
                        false => Err(message.unwrap_or_else(|| "the runtime refused".to_owned())),
                    };
                    self.answered(waiting, result);
                }
            }
            Message::Event { event, body, .. } => self.runtime_event(&event, body),
            Message::Request { .. } => {}
        }
    }

    fn answered(&mut self, waiting: Waiting, result: Result<Value, String>) {
        match waiting {
            Waiting::Started { seq, command } => {
                let init = result
                    .as_ref()
                    .ok()
                    .map(|body| parse::<InitializeBody>(body.clone()));
                self.editor
                    .respond(seq, &command, result.map(|_| Value::Null));
                if let Some(init) = init {
                    self.editor.event("initialized", json!({}));
                    self.output(format!("{}\n", text::describe(&init)));
                }
                for message in self.held.take().unwrap_or_default() {
                    self.runtime_message(message);
                }
            }
            Waiting::Breakpoints => match result.map(parse::<BreakpointsBody>) {
                Ok(verdict) if verdict.verified.iter().any(|v| !v) => {
                    self.output("warning: the runtime rejected a breakpoint\n");
                }
                Ok(_) => {}
                Err(message) => self.output(format!("breakpoints: {message}\n")),
            },
            Waiting::Silent => {
                if let Err(message) = result {
                    self.output(format!("{message}\n"));
                }
            }
            Waiting::StackTrace { seq, start, levels } => {
                let result = result.map(|body| {
                    let stack = parse(body);
                    self.note_code_position(&stack);
                    let locator = self.session.as_ref().map(|s| &s.locator);
                    self.lines.stack_frames(locator, &stack, start, levels)
                });
                self.editor.respond(seq, "stackTrace", result);
            }
            Waiting::Scopes { seq } => {
                let result = result.map(|body| self.refs.scopes(parse(body)));
                self.editor.respond(seq, "scopes", result);
            }
            Waiting::Resume { seq, command } => {
                let pause = command == "pause";
                let result = match result {
                    Ok(_) if pause => Ok(Value::Null),
                    Ok(_) => {
                        self.told_stopped = false;
                        self.refs.clear();
                        Ok(match command.as_str() {
                            "continue" => json!({ "allThreadsContinued": true }),
                            _ => Value::Null,
                        })
                    }
                    Err(message) => {
                        if !pause {
                            self.stepping = None;
                        }
                        Err(message)
                    }
                };
                self.editor.respond(seq, &command, result);
            }
            Waiting::Evaluate { seq, command } => {
                let result = result.map(|body| {
                    let shown = self
                        .session
                        .as_ref()
                        .map(|s| text::format(command, &body, &s.locator))
                        .unwrap_or_default();
                    json!({ "result": shown, "variablesReference": 0 })
                });
                self.editor.respond(seq, "evaluate", result);
            }
            Waiting::Custom { seq, command } => self.editor.respond(seq, &command, result),
            Waiting::CodeStop => {
                if let Ok(body) = result {
                    self.note_code_position(&parse(body));
                }
            }
            Waiting::StepCheck(stopped) => self.step_checked(stopped, result.ok().map(parse)),
            Waiting::Restep(stopped) => {
                if result.is_err() {
                    self.stepping = None;
                    self.show_stopped(stopped, true);
                }
            }
            Waiting::Swap { swap_for, locator } => self.swapped(swap_for, *locator, result),
        }
    }

    fn runtime_event(&mut self, name: &str, body: Value) {
        match name {
            event::STOPPED => {
                let stopped: StoppedBody = parse(body);
                self.refs.clear();
                if self.check_step(&stopped) {
                    return;
                }
                self.stepping = None;
                self.show_stopped(stopped, true);
            }
            event::CONTINUED if self.told_stopped => {
                self.told_stopped = false;
                self.refs.clear();
                self.editor.event(
                    "continued",
                    json!({ "threadId": THREAD, "allThreadsContinued": true }),
                );
            }
            event::OUTPUT => self.output(parse::<OutputBody>(body).text),
            _ => {}
        }
    }

    /// Tells the editor the story stopped; `fetch`: for a stop inside code, ask where it is.
    pub(super) fn show_stopped(&mut self, stopped: StoppedBody, fetch: bool) {
        self.told_stopped = true;
        self.refs.clear();
        if fetch {
            self.code_at = None;
            if stopped.in_code {
                let _ = self.forward(command::STACK_TRACE, Value::Null, Waiting::CodeStop);
            }
        }
        self.editor.event("stopped", stopped_event(&stopped));
        self.last_stop = Some(stopped);
    }
}

/// The DAP `stopped` event of a KDP stop. KDP's reasons are DAP's (`goto` is a move of the
/// story); the others pass as strings.
pub(super) fn stopped_event(stopped: &StoppedBody) -> Value {
    let description = stopped
        .description
        .clone()
        .or(match stopped.reason.as_str() {
            "choice" => Some("at a choice: select an arm with `kagari/push`".to_owned()),
            "end" => Some("the story ends".to_owned()),
            _ => None,
        });
    let mut body = json!({
        "reason": stopped.reason,
        "threadId": THREAD,
        "allThreadsStopped": true,
    });
    if let Some(object) = body.as_object_mut() {
        if let Some(description) = description {
            object.insert("description".to_owned(), Value::String(description));
        }
        if let Some(step) = &stopped.step {
            object.insert("text".to_owned(), Value::String(step.clone()));
        }
    }
    body
}
