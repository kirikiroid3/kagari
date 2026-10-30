//! `kagari debug`: the query engine's REPL, on the headless query engine, a game launched in a
//! window, or a running game (`--attach`). `swap` (and `--watch`, on a change of the project's
//! files) rebuilds the debug profile and hot swaps the game.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use kagari_debugger::Connection;
use kagari_debugger::client::text::{self, Parsed};
use kagari_debugger::client::{Backend, Locator, Watcher};
use kagari_debugger::headless::StartOptions;
use kagari_debugger::protocol::{
    BreakpointsBody, InitializeBody, Message, OutputBody, StoppedBody, command, event,
};
use serde_json::{Value, json};

use crate::debugger;
use crate::failure::Failure;

/// How often `--watch` looks at the project's files.
const WATCH_INTERVAL: Duration = Duration::from_millis(500);

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
    pub watch: bool,
}

pub fn run(options: &Options<'_>) -> Result<ExitCode, Failure> {
    let build = options.attach.is_none() && !options.no_build;
    let opened = debugger::open(options.manifest_path, options.target, build, &mut |line| {
        println!("{line}");
    })
    .map_err(Failure)?;
    let dir = opened.game_dir.clone();
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
                .map_or_else(debugger::default_runtime, Path::to_path_buf),
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
    let mut repl = Repl {
        conn: Connection::new(link),
        loc: opened.locator(),
        root: opened.source_root.clone(),
        pending: HashMap::new(),
        breakpoints: Vec::new(),
        manifest_path: options.manifest_path.map(Path::to_path_buf),
        target: options.target.map(str::to_owned),
        watcher: options.watch.then(|| opened.watcher()),
        polled: Instant::now(),
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
    /// `kagari/swap`; the locator reads the rebuilt game's sources.
    Swap(Box<Locator>),
}

struct Repl {
    conn: Connection,
    loc: Locator,
    root: PathBuf,
    pending: HashMap<i64, Waiting>,
    /// The `break` arguments, in order.
    breakpoints: Vec<String>,
    /// The project and target `swap` builds.
    manifest_path: Option<PathBuf>,
    target: Option<String>,
    /// `--watch`: the project's files, polled every [`WATCH_INTERVAL`].
    watcher: Option<Watcher>,
    polled: Instant,
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
            if !quitting && self.watched() {
                prompted = false;
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
            Ok(Parsed::Swap) => self.swap(),
            Err(error) => println!("error: {error}"),
        }
        false
    }

    /// `--watch`: at most every [`WATCH_INTERVAL`], looks at the project's files; a change
    /// rebuilds and swaps as `swap` does. Whether it did.
    fn watched(&mut self) -> bool {
        let Some(watcher) = self.watcher.as_mut() else {
            return false;
        };
        if self.polled.elapsed() < WATCH_INTERVAL {
            return false;
        }
        self.polled = Instant::now();
        if !watcher.changed() {
            return false;
        }
        println!("source changed: rebuilding");
        self.swap();
        true
    }

    /// `swap`: rebuilds the debug profile and asks the runtime to swap the new game in; the
    /// breakpoints are sent again once it has.
    fn swap(&mut self) {
        let opened = debugger::open(
            self.manifest_path.as_deref(),
            self.target.as_deref(),
            true,
            &mut |line| println!("{line}"),
        );
        let opened = match opened {
            Ok(opened) => opened,
            Err(error) => {
                println!("error: {error}");
                return;
            }
        };
        // The build has seen the files as they are.
        if let Some(watcher) = self.watcher.as_mut() {
            watcher.settle();
        }
        let seq = self.conn.request(command::SWAP, Value::Null);
        self.pending
            .insert(seq, Waiting::Swap(Box::new(opened.locator())));
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
                    println!("{}", text::describe(&init));
                }
                Some(Waiting::Silent) => {
                    let verdict: BreakpointsBody = serde_json::from_value(body).unwrap_or_default();
                    if verdict.verified.iter().any(|v| !v) {
                        println!("warning: the runtime rejected a breakpoint");
                    }
                }
                Some(Waiting::Swap(loc)) => {
                    self.loc = *loc;
                    println!("{}", text::format(command::SWAP, &body, &self.loc));
                    self.sync_breakpoints();
                }
                None => {}
            },
            Message::Event { event, body, .. } => match event.as_str() {
                event::STOPPED => {
                    let mut stopped: StoppedBody = serde_json::from_value(body).unwrap_or_default();
                    if stopped.reason == "swap" {
                        // The summary is the answer to `swap`.
                        stopped.description = None;
                    }
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
