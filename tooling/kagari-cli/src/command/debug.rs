//! `kagari debug`: the query engine's REPL, on the headless query engine, a game launched in a
//! window, or a running game (`--attach`).

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use kagari_debugger::Connection;
use kagari_debugger::headless::{BYTECODE_FILE, StartOptions};
use kagari_debugger::protocol::{
    BreakpointsBody, InitializeBody, Message, OutputBody, StoppedBody, command, event,
};
use serde_json::{Value, json};

use crate::debugger::backend::{self, Backend};
use crate::debugger::locate::Locator;
use crate::debugger::text::{self, Parsed};
use crate::failure::Failure;

/// The options of `kagari debug`.
pub struct Options<'a> {
    pub manifest_path: Option<&'a Path>,
    pub target: Option<&'a str>,
    pub attach: Option<&'a str>,
    pub window: bool,
    pub runtime: Option<&'a Path>,
    pub entry: Option<&'a str>,
    pub seed: u64,
    pub no_build: bool,
}

pub fn run(options: &Options<'_>) -> Result<ExitCode, Failure> {
    let (project, dir) =
        backend::project(options.manifest_path, options.target).map_err(Failure)?;
    if options.attach.is_none() && !options.no_build {
        backend::build_debug(&project, &dir, &mut |line| println!("{line}")).map_err(Failure)?;
    }
    let chosen = match options.attach {
        Some(address) => Backend::Attach {
            address: if address.contains(':') {
                address.to_owned()
            } else {
                format!("127.0.0.1:{address}")
            },
        },
        None if options.window => Backend::Window {
            runtime: options
                .runtime
                .map_or_else(backend::default_runtime, Path::to_path_buf),
            dir: dir.clone(),
            args: Vec::new(),
        },
        None => Backend::Headless {
            dir: dir.clone(),
            start: StartOptions {
                entry: options.entry.map(str::to_owned),
                seed: options.seed,
            },
        },
    };
    let mut attached = chosen.connect().map_err(Failure)?;
    let link = attached
        .link
        .take()
        .ok_or_else(|| Failure("no connection".to_owned()))?;
    let source_root = backend::source_root(&project);
    let mut repl = Repl {
        conn: Connection::new(link),
        loc: Locator::new(&source_root, Some(&dir.join(BYTECODE_FILE))),
        root: source_root.clone(),
        pending: HashMap::new(),
        breakpoints: Vec::new(),
    };
    repl.send(command::INITIALIZE, Value::Null);
    repl.run();
    attached.kill();
    Ok(ExitCode::SUCCESS)
}

/// What a waiting request was.
enum Waiting {
    /// Print the result of this command.
    Shown(&'static str),
    Initialize,
    /// A breakpoint update; reported by `break` itself.
    Silent,
}

struct Repl {
    conn: Connection,
    loc: Locator,
    root: PathBuf,
    pending: HashMap<i64, Waiting>,
    /// The `break` arguments, in order.
    breakpoints: Vec<String>,
}

impl Repl {
    fn send(&mut self, command: &'static str, args: Value) {
        let seq = self.conn.request(command, args);
        let waiting = match command {
            command::INITIALIZE => Waiting::Initialize,
            command::SET_STEP_BREAKPOINTS | command::SET_INSTRUCTION_BREAKPOINTS => Waiting::Silent,
            other => Waiting::Shown(other),
        };
        self.pending.insert(seq, waiting);
    }

    fn run(&mut self) {
        let (lines, input) = mpsc::channel();
        thread::spawn(move || {
            for line in std::io::stdin().lock().lines() {
                let Ok(line) = line else { break };
                if lines.send(line).is_err() {
                    break;
                }
            }
        });
        let mut eof = false;
        let mut quitting = false;
        let mut prompted = false;
        loop {
            while let Some(message) = self.conn.try_recv() {
                self.receive(message);
            }
            if self.conn.closed() {
                println!("the runtime is gone");
                return;
            }
            if self.pending.is_empty() {
                if eof || quitting {
                    break;
                }
                if !prompted {
                    print!("(kagari) ");
                    let _ = std::io::stdout().flush();
                    prompted = true;
                }
            }
            if eof || quitting {
                thread::sleep(Duration::from_millis(5));
                continue;
            }
            match input.recv_timeout(Duration::from_millis(20)) {
                Ok(line) => {
                    prompted = false;
                    quitting = self.command(&line);
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => eof = true,
            }
        }
        let bye = self.conn.request(command::DISCONNECT, Value::Null);
        // Wait briefly for the runtime to let go of the story.
        for _ in 0..100 {
            match self.conn.recv_timeout(Duration::from_millis(20)) {
                Some(Message::Response { request_seq, .. }) if request_seq == bye => break,
                Some(_) => {}
                None if self.conn.closed() => break,
                None => {}
            }
        }
    }

    /// Runs a command line; whether to quit.
    fn command(&mut self, line: &str) -> bool {
        match text::parse(line) {
            Ok(Parsed::Nothing) => {}
            Ok(Parsed::Help) => println!("{}", text::HELP),
            Ok(Parsed::Quit) => return true,
            Ok(Parsed::Request { command, args }) => self.send(command, args),
            Ok(Parsed::Breakpoints) => {
                if self.breakpoints.is_empty() {
                    println!("no breakpoints");
                }
                for (i, spec) in self.breakpoints.iter().enumerate() {
                    println!("{i}: {spec}");
                }
            }
            Ok(Parsed::Break(specs)) => {
                for spec in specs {
                    match self.resolve(&spec) {
                        Ok(what) => {
                            println!("breakpoint {}: {spec} ({what})", self.breakpoints.len());
                            self.breakpoints.push(spec);
                        }
                        Err(error) => println!("error: {error}"),
                    }
                }
                self.sync_breakpoints();
            }
            Ok(Parsed::Delete) => {
                self.breakpoints.clear();
                self.sync_breakpoints();
                println!("no breakpoints");
            }
            Err(error) => println!("error: {error}"),
        }
        false
    }

    /// What a `break` argument stops at, or why it cannot.
    fn resolve(&self, spec: &str) -> Result<String, String> {
        if spec.contains('#') {
            return Ok("a step".to_owned());
        }
        let (file, line) = file_line(spec)?;
        let found = self.loc.breakpoints_at(&self.path(file), line);
        if !found.steps.is_empty() {
            Ok(found.steps.join(", "))
        } else if !found.points.is_empty() {
            let functions: Vec<&str> = found.points.iter().map(|p| p.function.as_str()).collect();
            Ok(format!("code in {}", functions.join(", ")))
        } else {
            Err(format!(
                "{spec}: {}",
                found
                    .reason
                    .unwrap_or_else(|| "nothing to stop at".to_owned())
            ))
        }
    }

    fn path(&self, file: &str) -> PathBuf {
        let path = Path::new(file);
        if path.is_absolute() || path.exists() {
            path.to_path_buf()
        } else {
            self.root.join(path)
        }
    }

    /// Sends every breakpoint to the runtime.
    fn sync_breakpoints(&mut self) {
        let mut steps = Vec::new();
        let mut points = Vec::new();
        for spec in &self.breakpoints {
            if spec.contains('#') {
                steps.push(spec.clone());
            } else if let Ok((file, line)) = file_line(spec) {
                let found = self.loc.breakpoints_at(&self.path(file), line);
                steps.extend(found.steps);
                points.extend(found.points);
            }
        }
        self.send(command::SET_STEP_BREAKPOINTS, json!({ "steps": steps }));
        self.send(
            command::SET_INSTRUCTION_BREAKPOINTS,
            json!({ "points": points }),
        );
    }

    fn receive(&mut self, message: Message) {
        match message {
            Message::Response {
                request_seq,
                success,
                message,
                body,
                ..
            } => match self.pending.remove(&request_seq) {
                Some(_) if !success => {
                    println!("error: {}", message.unwrap_or_default());
                }
                Some(Waiting::Shown(command)) => {
                    let shown = text::format(command, &body, &self.loc);
                    if !shown.is_empty() {
                        println!("{shown}");
                    }
                }
                Some(Waiting::Initialize) => {
                    let init: InitializeBody = serde_json::from_value(body).unwrap_or_default();
                    println!("{}", describe(&init));
                }
                Some(Waiting::Silent) => {
                    let verdict: BreakpointsBody = serde_json::from_value(body).unwrap_or_default();
                    if verdict.verified.iter().any(|v| !v) {
                        println!("warning: the runtime rejected a breakpoint");
                    }
                }
                None => {}
            },
            Message::Event { event, body, .. } => match event.as_str() {
                event::STOPPED => {
                    let stopped: StoppedBody = serde_json::from_value(body).unwrap_or_default();
                    println!("{}", text::stopped(&stopped));
                }
                event::CONTINUED => println!("running"),
                event::OUTPUT => {
                    let output: OutputBody = serde_json::from_value(body).unwrap_or_default();
                    print!("{}", output.text);
                }
                _ => {}
            },
            Message::Request { .. } => {}
        }
    }
}

fn describe(init: &InitializeBody) -> String {
    let mode = if init.live {
        "a running game"
    } else {
        "the query engine"
    };
    let graph = match &init.artifact {
        None => "no debug artifact: `nav` and local names are unavailable".to_owned(),
        Some(a) if !a.graph => "no story graph: `nav` is unavailable".to_owned(),
        Some(a) if a.complete => "the story graph is complete".to_owned(),
        Some(a) => {
            let codes: Vec<&str> = a.incomplete.iter().map(|i| i.code.as_str()).collect();
            format!("the story graph is incomplete ({})", codes.join(", "))
        }
    };
    format!("debugging {mode}; {graph}. Type `help` for the commands.")
}

/// `file:line`.
fn file_line(spec: &str) -> Result<(&str, u32), String> {
    let (file, line) = spec
        .rsplit_once(':')
        .ok_or_else(|| format!("`{spec}` is neither a step ID nor `file:line`"))?;
    let line = line
        .parse()
        .map_err(|_| format!("`{spec}`: `{line}` is not a line number"))?;
    Ok((file, line))
}
