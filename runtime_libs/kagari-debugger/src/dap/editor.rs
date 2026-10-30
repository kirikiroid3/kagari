//! The editor's requests: answered at once, or forwarded to the runtime with a [`Waiting`]
//! that says how to answer when the runtime does.

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::answers::Waiting;
use super::swap::SwapFor;
use super::{Adapter, THREAD};
use crate::client::text::{self, Parsed};
use crate::protocol::{Message, command};

/// The `kagari/…` requests passed through unchanged.
const CUSTOM: [&str; 8] = [
    command::STACK,
    command::LS,
    command::GOTO,
    command::PUSH,
    command::POP,
    command::DETAIL,
    command::NAV,
    command::SEED,
];

/// The arguments the adapter reads, from whichever request has them.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct Args {
    lines_start_at1: Option<bool>,
    columns_start_at1: Option<bool>,
    /// `setBreakpoints`: the source, and its lines (`breakpoints[].line`, or the older
    /// `lines`); `setFunctionBreakpoints`: one entry per breakpoint.
    pub source: Source,
    pub breakpoints: Vec<Value>,
    pub lines: Vec<u32>,
    start_frame: Option<usize>,
    levels: Option<usize>,
    frame_id: usize,
    variables_reference: usize,
    expression: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct Source {
    pub path: Option<String>,
}

fn args<T: DeserializeOwned + Default>(arguments: Value) -> Result<T, String> {
    if arguments.is_null() {
        return Ok(T::default());
    }
    serde_json::from_value(arguments).map_err(|e| format!("malformed arguments: {e}"))
}

fn capabilities() -> Value {
    json!({
        "supportsConfigurationDoneRequest": true,
        "supportsEvaluateForHovers": false,
        "supportsTerminateRequest": true,
        "supportsSteppingGranularity": false,
        "supportsFunctionBreakpoints": false,
        "supportsConditionalBreakpoints": false,
        "exceptionBreakpointFilters": [],
    })
}

/// An editor request's answer: now, or later (`None`) when the runtime answers.
type Answer = Result<Option<Value>, String>;

impl Adapter<'_> {
    pub(super) fn editor_message(&mut self, message: Message) {
        let Message::Request {
            seq,
            command,
            arguments,
        } = message
        else {
            return;
        };
        match self.editor_request(seq, &command, arguments) {
            Ok(None) => {}
            Ok(Some(body)) => self.editor.respond(seq, &command, Ok(body)),
            Err(message) => self.editor.respond(seq, &command, Err(message)),
        }
    }

    /// Forwards a request whose answer the runtime gives.
    fn later(&mut self, command: &str, arguments: Value, waiting: Waiting) -> Answer {
        self.forward(command, arguments, waiting).map(|()| None)
    }

    fn editor_request(&mut self, seq: i64, name: &str, arguments: Value) -> Answer {
        if let Some(command) = CUSTOM.iter().find(|c| **c == name) {
            let waiting = Waiting::Custom {
                seq,
                command: name.to_owned(),
            };
            return self.later(command, arguments, waiting);
        }
        match name {
            "launch" | "attach" if self.session.is_some() => {
                return Err("a game is already being debugged".to_owned());
            }
            command::SWAP => return self.swap(SwapFor::Request(seq)).map(|()| None),
            "launch" => {
                let prepared = self.prepare_launch(args(arguments)?)?;
                return self.start(seq, name, prepared).map(|()| None);
            }
            "attach" => {
                let prepared = self.prepare_attach(args(arguments)?)?;
                return self.start(seq, name, prepared).map(|()| None);
            }
            _ => {}
        }
        let a: Args = args(arguments)?;
        match name {
            "initialize" => {
                self.lines.line_base = u32::from(a.lines_start_at1.unwrap_or(true));
                self.lines.column_base = u32::from(a.columns_start_at1.unwrap_or(true));
                Ok(Some(capabilities()))
            }
            "setBreakpoints" => self.set_breakpoints(a).map(Some),
            "setFunctionBreakpoints" => {
                let unverified = json!({
                    "verified": false,
                    "message": "function breakpoints are not supported: set a breakpoint on a line",
                });
                Ok(Some(
                    json!({ "breakpoints": vec![unverified; a.breakpoints.len()] }),
                ))
            }
            "setExceptionBreakpoints" => Ok(Some(json!({ "breakpoints": [] }))),
            "configurationDone" => Ok(Some(Value::Null)),
            "threads" => Ok(Some(
                json!({ "threads": [{ "id": THREAD, "name": "story" }] }),
            )),
            "stackTrace" => {
                let waiting = Waiting::StackTrace {
                    seq,
                    start: a.start_frame.unwrap_or(0),
                    levels: a.levels.filter(|&n| n > 0),
                };
                self.later(command::STACK_TRACE, Value::Null, waiting)
            }
            "scopes" => {
                let frame = json!({ "frame": a.frame_id });
                self.later(command::SCOPES, frame, Waiting::Scopes { seq })
            }
            "variables" => {
                let variables = self.refs.variables(a.variables_reference)?;
                Ok(Some(json!({ "variables": variables })))
            }
            "continue" | "next" | "stepIn" | "stepOut" | "pause" => self.resume(seq, name),
            "evaluate" => self.evaluate(seq, &a.expression),
            "disconnect" | "terminate" => {
                self.end_session();
                self.editor.respond(seq, name, Ok(Value::Null));
                if name == "terminate" {
                    self.editor.event("terminated", json!({}));
                }
                self.done = true;
                Ok(None)
            }
            other => Err(format!("`{other}` is not supported")),
        }
    }

    /// `continue`, `next`, `stepIn`, `stepOut`, `pause`. A `next` or `stepIn` from a stop
    /// inside code is repeated until it leaves the line.
    fn resume(&mut self, seq: i64, name: &str) -> Answer {
        let kdp = match name {
            "continue" => command::CONTINUE,
            "next" => command::NEXT,
            "stepIn" => command::STEP_IN,
            "stepOut" => command::STEP_OUT,
            _ => command::PAUSE,
        };
        let in_code = self.told_stopped && self.last_stop.as_ref().is_some_and(|s| s.in_code);
        self.stepping = match kdp {
            command::NEXT | command::STEP_IN if in_code => {
                Some(super::stepping::Stepping::new(kdp, self.code_at.clone()))
            }
            _ => None,
        };
        let waiting = Waiting::Resume {
            seq,
            command: name.to_owned(),
        };
        self.later(kdp, Value::Null, waiting)
    }

    /// A query command as text, answered as text.
    fn evaluate(&mut self, seq: i64, expression: &str) -> Answer {
        let shown = |text: &str| json!({ "result": text, "variablesReference": 0 });
        match text::parse(expression)? {
            Parsed::Request { command, args } => {
                self.later(command, args, Waiting::Evaluate { seq, command })
            }
            Parsed::Break(_) | Parsed::Breakpoints | Parsed::Delete => {
                Err("set breakpoints in the editor".to_owned())
            }
            Parsed::Swap => self.swap(SwapFor::Evaluate(seq)).map(|()| None),
            Parsed::Help => Ok(Some(shown(text::HELP))),
            Parsed::Quit => Err("end the debug session in the editor".to_owned()),
            Parsed::Nothing => Ok(Some(shown(""))),
        }
    }
}
