//! `kagari dap`: a debug adapter (the Debug Adapter Protocol) over stdin/stdout. The editor
//! speaks DAP with source lines; the runtime speaks the Kagari debug protocol (KDP) with step
//! IDs and byte offsets, over the same framing. One loop serves both connections: editor
//! requests become runtime requests (a pending map says what to do with each answer), runtime
//! events become editor events. Nothing but DAP is written to stdout.

use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc;
use std::time::Duration;

use kagari_debugger::headless::{BYTECODE_FILE, StartOptions};
use kagari_debugger::protocol::{
    BreakpointsBody, InitializeBody, Message, OutputBody, ScopesBody, StackBody, StoppedBody, Var,
    command, event,
};
use kagari_debugger::{Connection, Link};
use kagari_project::MANIFEST_FILE;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::debugger::backend::{self, Attached, Backend};
use crate::debugger::locate::{LineBreakpoints, Locator};
use crate::debugger::text::{self, Parsed};
use crate::failure::Failure;

/// The one thread a story has.
const THREAD: i64 = 1;
/// The most code steps that stay on one source line before a step is shown anyway.
const MAX_SILENT_STEPS: u32 = 200;
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

pub fn run() -> Result<ExitCode, Failure> {
    let (done, flushed) = mpsc::channel();
    let editor = Connection::new(Link::over(
        io::stdin(),
        Stdout {
            out: io::stdout(),
            done,
        },
    ));
    let mut adapter = Adapter::new(editor);
    adapter.run();
    // Let the writer thread write the last answers before the process ends.
    drop(adapter);
    let _ = flushed.recv_timeout(Duration::from_secs(2));
    Ok(ExitCode::SUCCESS)
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

/// `launch` arguments.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct LaunchArgs {
    manifest_path: Option<PathBuf>,
    cwd: Option<PathBuf>,
    target: Option<String>,
    mode: Option<String>,
    runtime: Option<PathBuf>,
    runtime_args: Vec<String>,
    entry: Option<String>,
    seed: Option<u64>,
    build_first: Option<bool>,
    stop_on_entry: bool,
}

/// `attach` arguments.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct AttachArgs {
    host: Option<String>,
    port: Option<u16>,
    manifest_path: Option<PathBuf>,
    cwd: Option<PathBuf>,
    target: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct InitializeArgs {
    lines_start_at1: Option<bool>,
    columns_start_at1: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct SetBreakpointsArgs {
    source: SourceArg,
    breakpoints: Vec<SourceBreakpoint>,
    lines: Vec<u32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct SourceArg {
    path: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct SourceBreakpoint {
    line: u32,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ListArgs {
    breakpoints: Vec<Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct StackTraceArgs {
    start_frame: Option<usize>,
    levels: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ScopesArgs {
    frame_id: usize,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct VariablesArgs {
    variables_reference: usize,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct EvaluateArgs {
    expression: String,
}

fn args<T: DeserializeOwned + Default>(arguments: Value) -> Result<T, String> {
    if arguments.is_null() {
        return Ok(T::default());
    }
    serde_json::from_value(arguments).map_err(|e| format!("malformed arguments: {e}"))
}

/// What to do with a runtime's answer.
enum Waiting {
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
}

impl Waiting {
    /// The editor request it answers.
    fn editor_request(&self) -> Option<(i64, &str)> {
        match self {
            Waiting::Started { seq, command }
            | Waiting::Resume { seq, command }
            | Waiting::Custom { seq, command } => Some((*seq, command)),
            Waiting::StackTrace { seq, .. } => Some((*seq, "stackTrace")),
            Waiting::Scopes { seq } => Some((*seq, "scopes")),
            Waiting::Evaluate { seq, .. } => Some((*seq, "evaluate")),
            Waiting::Breakpoints
            | Waiting::Silent
            | Waiting::CodeStop
            | Waiting::StepCheck(_)
            | Waiting::Restep(_) => None,
        }
    }
}

/// Where a stop inside code is, for line stepping.
#[derive(Clone, Debug, PartialEq, Eq)]
struct CodeAt {
    frames: usize,
    file: Option<String>,
    /// The source line, or the byte offset when the file cannot be read.
    line: Option<u32>,
}

/// A `next`/`stepIn` from a stop inside code, repeated while it stays on the line.
struct Stepping {
    command: &'static str,
    from: Option<CodeAt>,
    tries: u32,
}

/// A connected runtime.
struct Session {
    runtime: Connection,
    /// Kept alive: dropping it ends a launched game.
    attached: Attached,
    locator: Locator,
}

struct Adapter {
    editor: Connection,
    session: Option<Session>,
    pending: HashMap<i64, Waiting>,
    /// Runtime events held until the `launch`/`attach` is answered.
    held: Option<Vec<Message>>,
    lines_from_1: bool,
    columns_from_1: bool,
    /// The requested lines of each source file, with what each breaks at.
    breakpoints: HashMap<PathBuf, Vec<(u32, LineBreakpoints)>>,
    next_breakpoint: i64,
    /// `variablesReference` `n` is `refs[n - 1]`; cleared whenever the story moves.
    refs: Vec<Vec<Var>>,
    /// The editor was told the story is stopped.
    told_stopped: bool,
    last_stop: Option<StoppedBody>,
    code_at: Option<CodeAt>,
    stepping: Option<Stepping>,
    done: bool,
}

impl Adapter {
    fn new(editor: Connection) -> Adapter {
        Adapter {
            editor,
            session: None,
            pending: HashMap::new(),
            held: None,
            lines_from_1: true,
            columns_from_1: true,
            breakpoints: HashMap::new(),
            next_breakpoint: 0,
            refs: Vec::new(),
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
        self.editor.event(
            "output",
            json!({ "category": "console", "output": text.into() }),
        );
    }

    fn dap_line(&self, line: u32) -> u32 {
        if self.lines_from_1 {
            line
        } else {
            line.saturating_sub(1)
        }
    }

    fn dap_column(&self, column: u32) -> u32 {
        if self.columns_from_1 {
            column
        } else {
            column.saturating_sub(1)
        }
    }

    /// A 1-based line from the editor's line.
    fn editor_line(&self, line: u32) -> u32 {
        if self.lines_from_1 {
            line
        } else {
            line.saturating_add(1)
        }
    }

    /// Sends a runtime request.
    fn forward(&mut self, command: &str, arguments: Value, waiting: Waiting) -> Result<(), String> {
        let session = self
            .session
            .as_ref()
            .ok_or("no game is being debugged: launch or attach first")?;
        let seq = session.runtime.request(command, arguments);
        self.pending.insert(seq, waiting);
        Ok(())
    }

    fn editor_message(&mut self, message: Message) {
        let Message::Request {
            seq,
            command,
            arguments,
        } = message
        else {
            return;
        };
        let result = self.editor_request(seq, &command, arguments);
        match result {
            // Answered later, when the runtime answers.
            Ok(None) => {}
            Ok(Some(body)) => self.editor.respond(seq, &command, Ok(body)),
            Err(message) => self.editor.respond(seq, &command, Err(message)),
        }
    }

    /// Serves an editor request: its answer now, or `None` when the runtime answers it.
    fn editor_request(
        &mut self,
        seq: i64,
        name: &str,
        arguments: Value,
    ) -> Result<Option<Value>, String> {
        match name {
            "initialize" => {
                let a: InitializeArgs = args(arguments)?;
                self.lines_from_1 = a.lines_start_at1.unwrap_or(true);
                self.columns_from_1 = a.columns_start_at1.unwrap_or(true);
                Ok(Some(capabilities()))
            }
            "launch" => self.launch(seq, name, args(arguments)?).map(|()| None),
            "attach" => self.attach(seq, name, args(arguments)?).map(|()| None),
            "setBreakpoints" => self.set_breakpoints(args(arguments)?).map(Some),
            "setFunctionBreakpoints" => {
                let a: ListArgs = args(arguments)?;
                let unverified: Vec<Value> = a
                    .breakpoints
                    .iter()
                    .map(|_| {
                        json!({
                            "verified": false,
                            "message": "function breakpoints are not supported: \
                                        set a breakpoint on a line",
                        })
                    })
                    .collect();
                Ok(Some(json!({ "breakpoints": unverified })))
            }
            "setExceptionBreakpoints" => Ok(Some(json!({ "breakpoints": [] }))),
            "configurationDone" => Ok(Some(Value::Null)),
            "threads" => Ok(Some(json!({
                "threads": [{ "id": THREAD, "name": "story" }],
            }))),
            "stackTrace" => {
                let a: StackTraceArgs = args(arguments)?;
                self.forward(
                    command::STACK_TRACE,
                    Value::Null,
                    Waiting::StackTrace {
                        seq,
                        start: a.start_frame.unwrap_or(0),
                        levels: a.levels.filter(|&n| n > 0),
                    },
                )
                .map(|()| None)
            }
            "scopes" => {
                let a: ScopesArgs = args(arguments)?;
                self.forward(
                    command::SCOPES,
                    json!({ "frame": a.frame_id }),
                    Waiting::Scopes { seq },
                )
                .map(|()| None)
            }
            "variables" => {
                let a: VariablesArgs = args(arguments)?;
                let vars = a
                    .variables_reference
                    .checked_sub(1)
                    .and_then(|i| self.refs.get(i))
                    .cloned()
                    .ok_or("the variables are gone: the story moved")?;
                let variables: Vec<Value> = vars.into_iter().map(|var| self.var(var)).collect();
                Ok(Some(json!({ "variables": variables })))
            }
            "continue" | "next" | "stepIn" | "stepOut" | "pause" => {
                self.resume(seq, name).map(|()| None)
            }
            "evaluate" => self.evaluate(seq, args(arguments)?),
            "disconnect" | "terminate" => {
                self.end_session();
                self.editor.respond(seq, name, Ok(Value::Null));
                if name == "terminate" {
                    self.editor.event("terminated", json!({}));
                }
                self.done = true;
                Ok(None)
            }
            custom => match CUSTOM.iter().find(|c| **c == custom) {
                Some(command) => self
                    .forward(
                        command,
                        arguments,
                        Waiting::Custom {
                            seq,
                            command: custom.to_owned(),
                        },
                    )
                    .map(|()| None),
                None => Err(format!("`{custom}` is not supported")),
            },
        }
    }

    fn launch(&mut self, seq: i64, name: &str, a: LaunchArgs) -> Result<(), String> {
        if self.session.is_some() {
            return Err("a game is already being debugged".to_owned());
        }
        let cwd = a.cwd.as_deref();
        let manifest = manifest_path(cwd, a.manifest_path.as_deref())?;
        let (project, dir) = backend::project(manifest.as_deref(), a.target.as_deref())?;
        let window = match a.mode.as_deref() {
            None | Some("window") => true,
            Some("headless") => false,
            Some(other) => {
                return Err(format!("`mode` is `window` or `headless`, not `{other}`"));
            }
        };
        if a.build_first.unwrap_or(true) {
            let editor = &self.editor;
            let mut out = |line: &str| {
                editor.event(
                    "output",
                    json!({ "category": "console", "output": format!("{line}\n") }),
                );
            };
            backend::build_debug(&project, &dir, &mut out)?;
        }
        let chosen = if window {
            let runtime = match a.runtime {
                Some(runtime) => match cwd {
                    Some(cwd) if runtime.is_relative() && cwd.join(&runtime).exists() => {
                        cwd.join(runtime)
                    }
                    _ => runtime,
                },
                None => backend::default_runtime(),
            };
            Backend::Window {
                runtime,
                dir: dir.clone(),
                args: a.runtime_args,
            }
        } else {
            Backend::Headless {
                dir: dir.clone(),
                start: StartOptions {
                    entry: a.entry,
                    seed: a.seed.unwrap_or(1),
                },
            }
        };
        let locator = Locator::new(
            &backend::source_root(&project),
            Some(&dir.join(BYTECODE_FILE)),
        );
        self.start(seq, name, chosen, locator, window && a.stop_on_entry)
    }

    fn attach(&mut self, seq: i64, name: &str, a: AttachArgs) -> Result<(), String> {
        if self.session.is_some() {
            return Err("a game is already being debugged".to_owned());
        }
        let port = a.port.ok_or("`attach` needs the game's `port`")?;
        let host = a.host.unwrap_or_else(|| "127.0.0.1".to_owned());
        let address = if host.contains(':') && !host.starts_with('[') {
            format!("[{host}]:{port}")
        } else {
            format!("{host}:{port}")
        };
        let manifest = manifest_path(a.cwd.as_deref(), a.manifest_path.as_deref())?;
        let (project, dir) = backend::project(manifest.as_deref(), a.target.as_deref())?;
        let locator = Locator::new(
            &backend::source_root(&project),
            Some(&dir.join(BYTECODE_FILE)),
        );
        self.start(seq, name, Backend::Attach { address }, locator, false)
    }

    /// Connects to the runtime and asks it to `initialize`; the editor's request is answered
    /// with the runtime's answer.
    fn start(
        &mut self,
        seq: i64,
        name: &str,
        chosen: Backend,
        locator: Locator,
        pause: bool,
    ) -> Result<(), String> {
        let mut attached = chosen.connect()?;
        let link = attached
            .link
            .take()
            .ok_or("the runtime has no connection")?;
        let runtime = Connection::new(link);
        let init = runtime.request(command::INITIALIZE, Value::Null);
        self.pending.insert(
            init,
            Waiting::Started {
                seq,
                command: name.to_owned(),
            },
        );
        if pause {
            let paused = runtime.request(command::PAUSE, Value::Null);
            self.pending.insert(paused, Waiting::Silent);
        }
        self.held = Some(Vec::new());
        self.session = Some(Session {
            runtime,
            attached,
            locator,
        });
        Ok(())
    }

    fn set_breakpoints(&mut self, a: SetBreakpointsArgs) -> Result<Value, String> {
        let path = a
            .source
            .path
            .ok_or("`setBreakpoints` needs a source with a path")?;
        let path = PathBuf::from(path);
        let lines: Vec<u32> = if a.breakpoints.is_empty() {
            a.lines
        } else {
            a.breakpoints.iter().map(|b| b.line).collect()
        };
        let found: Vec<(u32, LineBreakpoints)> = lines
            .iter()
            .map(|&line| {
                let at = match &self.session {
                    Some(session) => session
                        .locator
                        .breakpoints_at(&path, self.editor_line(line)),
                    None => LineBreakpoints {
                        reason: Some("no game is being debugged".to_owned()),
                        ..LineBreakpoints::default()
                    },
                };
                (line, at)
            })
            .collect();
        let mut answer = Vec::new();
        for (line, at) in &found {
            self.next_breakpoint = self.next_breakpoint.saturating_add(1);
            answer.push(breakpoint(self.next_breakpoint, *line, &path, at));
        }
        let key = path.canonicalize().unwrap_or(path);
        if found.is_empty() {
            self.breakpoints.remove(&key);
        } else {
            self.breakpoints.insert(key, found);
        }
        if self.session.is_some() {
            self.sync_breakpoints()?;
        }
        Ok(json!({ "breakpoints": answer }))
    }

    /// Sends the breakpoints of every file to the runtime.
    fn sync_breakpoints(&mut self) -> Result<(), String> {
        let mut steps = Vec::new();
        let mut seen_steps = HashSet::new();
        let mut points = Vec::new();
        let mut seen_points = HashSet::new();
        for (_, at) in self.breakpoints.values().flatten() {
            for step in &at.steps {
                if seen_steps.insert(step.clone()) {
                    steps.push(step.clone());
                }
            }
            for point in &at.points {
                if seen_points.insert((point.function.clone(), point.word)) {
                    points.push(point.clone());
                }
            }
        }
        self.forward(
            command::SET_STEP_BREAKPOINTS,
            json!({ "steps": steps }),
            Waiting::Breakpoints,
        )?;
        self.forward(
            command::SET_INSTRUCTION_BREAKPOINTS,
            json!({ "points": points }),
            Waiting::Breakpoints,
        )
    }

    fn resume(&mut self, seq: i64, name: &str) -> Result<(), String> {
        let kdp = match name {
            "continue" => command::CONTINUE,
            "next" => command::NEXT,
            "stepIn" => command::STEP_IN,
            "stepOut" => command::STEP_OUT,
            _ => command::PAUSE,
        };
        let in_code = self.told_stopped && self.last_stop.as_ref().is_some_and(|s| s.in_code);
        self.stepping = match kdp {
            command::NEXT | command::STEP_IN if in_code => Some(Stepping {
                command: kdp,
                from: self.code_at.clone(),
                tries: 0,
            }),
            _ => None,
        };
        self.forward(
            kdp,
            Value::Null,
            Waiting::Resume {
                seq,
                command: name.to_owned(),
            },
        )
    }

    fn evaluate(&mut self, seq: i64, a: EvaluateArgs) -> Result<Option<Value>, String> {
        let shown = |text: &str| json!({ "result": text, "variablesReference": 0 });
        match text::parse(&a.expression)? {
            Parsed::Request { command, args } => self
                .forward(command, args, Waiting::Evaluate { seq, command })
                .map(|()| None),
            Parsed::Break(_) | Parsed::Breakpoints | Parsed::Delete => {
                Err("set breakpoints in the editor".to_owned())
            }
            Parsed::Help => Ok(Some(shown(text::HELP))),
            Parsed::Quit => Err("end the debug session in the editor".to_owned()),
            Parsed::Nothing => Ok(Some(shown(""))),
        }
    }

    /// A DAP variable; one with children gets a fresh reference.
    fn var(&mut self, var: Var) -> Value {
        let reference = if var.children.is_empty() {
            0
        } else {
            self.reference(var.children)
        };
        json!({
            "name": var.name,
            "value": var.value,
            "variablesReference": reference,
        })
    }

    fn reference(&mut self, vars: Vec<Var>) -> usize {
        self.refs.push(vars);
        self.refs.len()
    }

    fn runtime_message(&mut self, message: Message) {
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
                    let result = if success {
                        Ok(body)
                    } else {
                        Err(message.unwrap_or_else(|| "the runtime refused".to_owned()))
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
                match &result {
                    Ok(body) => {
                        let init: InitializeBody =
                            serde_json::from_value(body.clone()).unwrap_or_default();
                        self.editor.respond(seq, &command, Ok(Value::Null));
                        self.editor.event("initialized", json!({}));
                        self.output(format!("{}\n", crate::command::debug::describe(&init)));
                    }
                    Err(message) => self.editor.respond(seq, &command, Err(message.clone())),
                }
                for message in self.held.take().unwrap_or_default() {
                    self.runtime_message(message);
                }
            }
            Waiting::Breakpoints => match result {
                Ok(body) => {
                    let verdict: BreakpointsBody = serde_json::from_value(body).unwrap_or_default();
                    if verdict.verified.iter().any(|v| !v) {
                        self.output("warning: the runtime rejected a breakpoint\n");
                    }
                }
                Err(message) => self.output(format!("breakpoints: {message}\n")),
            },
            Waiting::Silent => {
                if let Err(message) = result {
                    self.output(format!("{message}\n"));
                }
            }
            Waiting::StackTrace { seq, start, levels } => {
                let result = result.map(|body| {
                    let stack: StackBody = serde_json::from_value(body).unwrap_or_default();
                    self.note_code_position(&stack);
                    self.stack_frames(&stack, start, levels)
                });
                self.editor.respond(seq, "stackTrace", result);
            }
            Waiting::Scopes { seq } => {
                let result = result.map(|body| {
                    let scopes: ScopesBody = serde_json::from_value(body).unwrap_or_default();
                    let scopes: Vec<Value> = scopes
                        .scopes
                        .into_iter()
                        .map(|scope| {
                            json!({
                                "name": scope.name,
                                "variablesReference": self.reference(scope.vars),
                                "expensive": false,
                            })
                        })
                        .collect();
                    json!({ "scopes": scopes })
                });
                self.editor.respond(seq, "scopes", result);
            }
            Waiting::Resume { seq, command } => {
                let result = match result {
                    Ok(_) if command == "pause" => Ok(Value::Null),
                    Ok(_) => {
                        self.told_stopped = false;
                        self.refs.clear();
                        Ok(if command == "continue" {
                            json!({ "allThreadsContinued": true })
                        } else {
                            Value::Null
                        })
                    }
                    Err(message) => {
                        if command != "pause" {
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
                    let stack: StackBody = serde_json::from_value(body).unwrap_or_default();
                    self.note_code_position(&stack);
                }
            }
            Waiting::StepCheck(stopped) => {
                let at = result.ok().map(|body| {
                    let stack: StackBody = serde_json::from_value(body).unwrap_or_default();
                    self.code_position(&stack)
                });
                let again = match (&mut self.stepping, &at) {
                    (Some(stepping), Some(at))
                        if stepping.from.as_ref() == Some(at)
                            && stepping.tries < MAX_SILENT_STEPS =>
                    {
                        stepping.tries = stepping.tries.saturating_add(1);
                        Some(stepping.command)
                    }
                    _ => None,
                };
                match again {
                    Some(step) => {
                        let sent =
                            self.forward(step, Value::Null, Waiting::Restep(stopped.clone()));
                        if sent.is_err() {
                            self.stepping = None;
                            self.show_stopped(stopped, at.is_none());
                        }
                    }
                    None => {
                        self.stepping = None;
                        let fetch = at.is_none();
                        self.code_at = at;
                        self.show_stopped(stopped, fetch);
                    }
                }
            }
            Waiting::Restep(stopped) => {
                if result.is_err() {
                    self.stepping = None;
                    self.show_stopped(stopped, true);
                }
            }
        }
    }

    fn runtime_event(&mut self, name: &str, body: Value) {
        match name {
            event::STOPPED => {
                let stopped: StoppedBody = serde_json::from_value(body).unwrap_or_default();
                self.refs.clear();
                if self.stepping.is_some() && stopped.in_code && stopped.reason == "step" {
                    let sent = self.forward(
                        command::STACK_TRACE,
                        Value::Null,
                        Waiting::StepCheck(stopped.clone()),
                    );
                    if sent.is_ok() {
                        return;
                    }
                }
                self.stepping = None;
                self.show_stopped(stopped, true);
            }
            event::CONTINUED => {
                if self.told_stopped {
                    self.told_stopped = false;
                    self.refs.clear();
                    self.editor.event(
                        "continued",
                        json!({ "threadId": THREAD, "allThreadsContinued": true }),
                    );
                }
            }
            event::OUTPUT => {
                let output: OutputBody = serde_json::from_value(body).unwrap_or_default();
                self.output(output.text);
            }
            _ => {}
        }
    }

    /// Tells the editor the story stopped; `fetch`: for a stop inside code, ask where it is.
    fn show_stopped(&mut self, stopped: StoppedBody, fetch: bool) {
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

    /// Remembers where a stop inside code is, from a stack taken while it holds.
    fn note_code_position(&mut self, stack: &StackBody) {
        if !self.told_stopped || !self.last_stop.as_ref().is_some_and(|s| s.in_code) {
            return;
        }
        let at = self.code_position(stack);
        if let Some(stepping) = &mut self.stepping
            && stepping.from.is_none()
        {
            stepping.from = Some(at.clone());
        }
        self.code_at = Some(at);
    }

    fn code_position(&self, stack: &StackBody) -> CodeAt {
        let top = stack.frames.first();
        let file = top.and_then(|f| f.file.clone());
        let line = top.and_then(|frame| {
            let offset = frame.offset?;
            let located = match (&self.session, frame.file.as_deref()) {
                (Some(session), Some(file)) => session.locator.locate(file, offset),
                _ => None,
            };
            Some(located.map_or(offset, |at| at.line))
        });
        CodeAt {
            frames: stack.frames.len(),
            file,
            line,
        }
    }

    fn stack_frames(&self, stack: &StackBody, start: usize, levels: Option<usize>) -> Value {
        let frames: Vec<Value> = stack
            .frames
            .iter()
            .enumerate()
            .skip(start)
            .take(levels.unwrap_or(usize::MAX))
            .map(|(id, frame)| {
                let name = match &frame.step {
                    Some(step) => format!("{} — {step}", frame.function),
                    None => frame.function.clone(),
                };
                let located = match (&self.session, frame.file.as_deref(), frame.offset) {
                    (Some(session), Some(file), Some(offset)) => {
                        session.locator.locate(file, offset)
                    }
                    _ => None,
                };
                match located {
                    Some(at) => json!({
                        "id": id,
                        "name": name,
                        "source": {
                            "name": at
                                .path
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned()),
                            "path": at.path,
                        },
                        "line": self.dap_line(at.line),
                        "column": self.dap_column(at.column),
                    }),
                    None => json!({
                        "id": id,
                        "name": name,
                        "line": 0,
                        "column": 0,
                        "presentationHint": "subtle",
                    }),
                }
            })
            .collect();
        json!({ "stackFrames": frames, "totalFrames": stack.frames.len() })
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
                Some(_) => {}
                None if session.runtime.closed() => break,
                None => {}
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

/// A DAP breakpoint for the requested `line` (as the editor counts).
fn breakpoint(id: i64, line: u32, path: &Path, at: &LineBreakpoints) -> Value {
    let verified = !at.steps.is_empty() || !at.points.is_empty();
    let mut out = json!({
        "id": id,
        "verified": verified,
        "line": line,
        "source": { "path": path },
    });
    if !verified && let Some(object) = out.as_object_mut() {
        object.insert(
            "message".to_owned(),
            Value::String(
                at.reason
                    .clone()
                    .unwrap_or_else(|| "nothing to stop at".to_owned()),
            ),
        );
    }
    out
}

/// The DAP `stopped` event of a KDP stop.
fn stopped_event(stopped: &StoppedBody) -> Value {
    let description = stopped
        .description
        .clone()
        .or_else(|| match stopped.reason.as_str() {
            "choice" => Some("at a choice: select an arm with `kagari/push`".to_owned()),
            "end" => Some("the story ends".to_owned()),
            _ => None,
        });
    // KDP's reasons are DAP's (`goto` is a move of the story); the others pass as strings.
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use kagari_debugger::protocol::InstructionPoint;

    #[test]
    fn an_unverified_line_says_why() {
        let at = LineBreakpoints {
            reason: Some("no code on this line".to_owned()),
            ..LineBreakpoints::default()
        };
        let bp = breakpoint(3, 7, Path::new("/game/src/main.kis"), &at);
        assert_eq!(bp["verified"], false);
        assert_eq!(bp["line"], 7);
        assert_eq!(bp["message"], "no code on this line");

        let at = LineBreakpoints {
            points: vec![InstructionPoint {
                function: "crate::Start".to_owned(),
                word: 4,
            }],
            ..LineBreakpoints::default()
        };
        let bp = breakpoint(4, 8, Path::new("/game/src/main.kis"), &at);
        assert_eq!(bp["verified"], true);
        assert!(bp.get("message").is_none());
    }

    #[test]
    fn stops_keep_their_reason_and_explain_choices() {
        let body = stopped_event(&StoppedBody {
            reason: "choice".to_owned(),
            step: Some("crate::Start#2".to_owned()),
            ..StoppedBody::default()
        });
        assert_eq!(body["reason"], "choice");
        assert_eq!(body["threadId"], THREAD);
        assert!(body["description"].as_str().unwrap().contains("choice"));
    }

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
