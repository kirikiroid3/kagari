//! The runtime side of the protocol: requests to [`Debugger`] commands, stops to events.
//!
//! A **live** server runs inside a runtime the player plays (the desktop window): it is polled
//! once per frame, `continue` releases the hold and the player plays on, and the engine reports
//! the stops it makes by itself (step breakpoints, pauses). A **headless** server (the query
//! engine) serves until the client disconnects, and `continue` drives the story itself.
//! Either way a stop inside code blocks in the VM's hook and serves the requests that make
//! sense there ([`serve_code_stop`]).

use std::rc::Rc;

use kagari_engine::debug::StopReason;
use kagari_vm::HookStop;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::frames::{Artifact, frame_info, frame_scopes, story_vars};
use crate::hook::Hook;
use crate::protocol::{
    InstructionBreakpointsArgs, Message, NavArgs, PushArgs, ScopesArgs, ScopesBody, SeedArgs,
    StackBody, StepArgs, StepBreakpointsArgs, StoppedBody, command, event,
};
use crate::session::{Debugger, Run, Target, parse_step};
use crate::transport::{Connection, Link};
use crate::values::Names;

/// A debugger attached to a runtime.
pub struct Server {
    conn: Rc<Connection>,
    debugger: Debugger,
    live: bool,
    /// The client was told the story is stopped.
    stopped: bool,
    attached: bool,
}

impl Server {
    /// Attaches to the engine of `t`: installs the instruction hook. `live` for a runtime the
    /// player plays.
    pub fn attach(
        link: Link,
        artifact: Option<Artifact>,
        live: bool,
        t: &mut Target<'_>,
    ) -> Server {
        let conn = Rc::new(Connection::new(link));
        let artifact = artifact.map(Rc::new);
        let debugger = Debugger::new(t.engine, artifact.clone());
        t.engine.set_instr_hook(Some(Box::new(Hook {
            state: debugger.hook.clone(),
            conn: conn.clone(),
            artifact,
        })));
        Server {
            conn,
            debugger,
            live,
            stopped: !live,
            attached: true,
        }
    }

    pub fn attached(&self) -> bool {
        self.attached
    }

    pub fn debugger(&mut self) -> &mut Debugger {
        &mut self.debugger
    }

    /// Tells the client the story stopped.
    pub fn send_stopped(&mut self, body: StoppedBody) {
        self.stopped = true;
        self.conn.event(event::STOPPED, body);
    }

    /// For a live runtime, once per frame: reports the engine's own stops and the end of a
    /// hold, then serves the waiting requests.
    pub fn poll(&mut self, t: &mut Target<'_>) {
        if !self.attached {
            return;
        }
        self.sync_hook(t);
        self.engine_events(t);
        while self.attached
            && let Some(message) = self.conn.try_recv()
        {
            self.handle(t, message);
        }
        if self.conn.closed() {
            self.detach(t);
        }
    }

    /// For the headless query engine: serves until the client disconnects.
    pub fn serve(&mut self, t: &mut Target<'_>) {
        while self.attached {
            match self.conn.recv() {
                Some(message) => self.handle(t, message),
                None => self.detach(t),
            }
        }
    }

    /// Applies what a stop inside code could not: step breakpoints, a disconnect.
    fn sync_hook(&mut self, t: &mut Target<'_>) {
        let (pending, detached) = {
            let mut state = self.debugger.hook.borrow_mut();
            (
                state.pending_steps.take(),
                std::mem::take(&mut state.detached),
            )
        };
        if let Some(steps) = pending {
            self.debugger.set_step_breakpoints(t, &steps);
        }
        if detached {
            self.detach(t);
        }
    }

    fn engine_events(&mut self, t: &mut Target<'_>) {
        if let Some(stop) = t.engine.debug_take_stop() {
            let code_step = {
                let mut hook = self.debugger.hook.borrow_mut();
                let code_step = hook.story_step();
                hook.clear_step();
                code_step
            };
            let reason = match (code_step, stop.reason) {
                (true, _) => "step",
                (false, StopReason::Breakpoint) => "breakpoint",
                (false, StopReason::Pause) => "pause",
            };
            let body = self.debugger.stopped(t, reason, None);
            self.send_stopped(body);
        } else if self.live && self.stopped && !t.engine.debug_held() {
            // A load or a new game by the player ended the hold.
            self.stopped = false;
            self.conn.event(event::CONTINUED, Value::Null);
        }
    }

    /// Detaches: removes the hook and the breakpoints and releases the story.
    fn detach(&mut self, t: &mut Target<'_>) {
        if !self.attached {
            return;
        }
        self.attached = false;
        t.engine.set_instr_hook(None);
        t.engine.debug_set_breakpoints([]);
        t.engine.debug_release();
    }

    fn handle(&mut self, t: &mut Target<'_>, message: Message) {
        let Message::Request {
            seq,
            command,
            arguments,
        } = message
        else {
            return;
        };
        let respond = |conn: &Connection, result: Result<Value, String>| {
            conn.respond(seq, &command, result);
        };
        match command.as_str() {
            command::INITIALIZE => {
                let body = self.debugger.initialize(t, self.live);
                respond(&self.conn, json(body));
            }
            command::DISCONNECT => {
                respond(&self.conn, Ok(Value::Null));
                self.detach(t);
            }
            command::SET_STEP_BREAKPOINTS => {
                let result = args::<StepBreakpointsArgs>(arguments)
                    .and_then(|a| json(self.debugger.set_step_breakpoints(t, &a.steps)));
                respond(&self.conn, result);
            }
            command::SET_INSTRUCTION_BREAKPOINTS => {
                let result = args::<InstructionBreakpointsArgs>(arguments)
                    .and_then(|a| json(self.debugger.set_instruction_breakpoints(&a.points)));
                respond(&self.conn, result);
            }
            command::CONTINUE if self.live => {
                self.debugger.hook.borrow_mut().clear_step();
                t.engine.debug_release();
                respond(&self.conn, Ok(Value::Null));
                self.stopped = false;
                self.conn.event(event::CONTINUED, Value::Null);
            }
            command::CONTINUE | command::NEXT | command::STEP_IN | command::STEP_OUT => {
                let how = match command.as_str() {
                    command::CONTINUE => Run::Continue,
                    command::NEXT => Run::Next,
                    command::STEP_IN => Run::StepIn,
                    _ => Run::StepOut,
                };
                if self.live && !t.engine.debug_held() {
                    respond(&self.conn, Err("the story is not stopped".to_owned()));
                    return;
                }
                respond(&self.conn, Ok(Value::Null));
                self.stopped = false;
                self.conn.event(event::CONTINUED, Value::Null);
                let conn = self.conn.clone();
                let mut interrupted = || match conn.take_request(command::PAUSE) {
                    Some(seq) => {
                        conn.respond(seq, command::PAUSE, Ok(Value::Null));
                        true
                    }
                    None => false,
                };
                let body = match self.debugger.run(t, how, &mut interrupted) {
                    Ok(body) => body,
                    Err(error) => self.debugger.stopped(t, "error", Some(error)),
                };
                self.sync_hook(t);
                self.send_stopped(body);
            }
            command::PAUSE => {
                respond(&self.conn, Ok(Value::Null));
                if self.live {
                    t.engine.debug_pause();
                    self.engine_events(t);
                } else {
                    let body = self.debugger.stopped(t, "pause", None);
                    self.send_stopped(body);
                }
            }
            command::STACK_TRACE | command::STACK => {
                respond(&self.conn, self.debugger.stack(t).and_then(json));
            }
            command::SCOPES => {
                let result = args::<ScopesArgs>(arguments)
                    .and_then(|a| self.debugger.scopes(t, a.frame))
                    .and_then(json);
                respond(&self.conn, result);
            }
            command::LS => {
                let result = args::<crate::protocol::LsArgs>(arguments)
                    .and_then(|a| self.debugger.ls(t, a.head, a.tail))
                    .and_then(json);
                respond(&self.conn, result);
            }
            command::DETAIL => {
                let result = args::<StepArgs>(arguments)
                    .and_then(|a| self.debugger.detail(t, a.step.as_deref()))
                    .and_then(json);
                respond(&self.conn, result);
            }
            command::NAV => {
                let result = args::<NavArgs>(arguments)
                    .and_then(|a| self.debugger.nav(t, &a.step, a.seeds))
                    .and_then(json);
                respond(&self.conn, result);
            }
            command::GOTO | command::PUSH | command::POP | command::SEED => {
                let result = match command.as_str() {
                    command::GOTO => args::<StepArgs>(arguments).and_then(|a| {
                        let step = a.step.ok_or("`goto` needs a step")?;
                        self.debugger.goto(t, &step).and_then(json)
                    }),
                    command::PUSH => args::<PushArgs>(arguments)
                        .and_then(|a| self.debugger.push(t, &a.arms).and_then(json)),
                    command::POP => self.debugger.pop(t).and_then(json),
                    _ => args::<SeedArgs>(arguments)
                        .and_then(|a| self.debugger.seed(t, a.seed).and_then(json)),
                };
                let moved = result.is_ok() || command == command::PUSH;
                respond(&self.conn, result);
                self.sync_hook(t);
                // A move (even a `push` that stopped early) holds the story where it is now.
                if moved && t.engine.debug_held() {
                    let body = self.debugger.stopped(t, "goto", None);
                    self.send_stopped(body);
                }
            }
            _ => respond(&self.conn, Err(format!("unknown request `{command}`"))),
        }
    }
}

fn json(body: impl Serialize) -> Result<Value, String> {
    serde_json::to_value(body).map_err(|e| e.to_string())
}

fn args<T: DeserializeOwned + Default>(arguments: Value) -> Result<T, String> {
    if arguments.is_null() {
        return Ok(T::default());
    }
    serde_json::from_value(arguments).map_err(|e| format!("malformed arguments: {e}"))
}

/// A stop inside code: tells the client, then serves until it goes on. Moves of the story
/// are refused here: the VM is in the middle of running it.
pub(crate) fn serve_code_stop(hook: &Hook, stop: &HookStop<'_>, breakpoint: bool) {
    let names = Names {
        program: stop.program,
        types: stop.types,
    };
    let description = stop.frames.last().map(|top| {
        format!(
            "{} at {}",
            stop.program.function_path(top.func),
            stop.program
                .location(top.func, top.pc)
                .unwrap_or_else(|| "an unknown location".to_owned())
        )
    });
    hook.conn.event(
        event::STOPPED,
        StoppedBody {
            reason: if breakpoint { "breakpoint" } else { "step" }.to_owned(),
            step: None,
            description,
            in_code: true,
        },
    );
    let artifact: Option<&Artifact> = hook.artifact.as_deref();
    loop {
        let Some(message) = hook.conn.recv() else {
            // The client is gone: run on without stopping.
            if let Ok(mut state) = hook.state.try_borrow_mut() {
                state.armed = false;
                state.detached = true;
            }
            return;
        };
        let Message::Request {
            seq,
            command,
            arguments,
        } = message
        else {
            continue;
        };
        let respond = |result: Result<Value, String>| hook.conn.respond(seq, &command, result);
        match command.as_str() {
            command::STACK_TRACE | command::STACK => {
                let frames = stop
                    .frames
                    .iter()
                    .rev()
                    .map(|frame| frame_info(artifact, names, frame, None))
                    .collect();
                respond(json(StackBody { frames }));
            }
            command::SCOPES => {
                let result = args::<ScopesArgs>(arguments).and_then(|a| {
                    let frame = stop
                        .frames
                        .len()
                        .checked_sub(a.frame.saturating_add(1))
                        .and_then(|i| stop.frames.get(i))
                        .ok_or("no such frame")?;
                    let mut scopes = frame_scopes(artifact, names, frame);
                    if let Some(statics) = stop.statics {
                        scopes.push(story_vars(names, statics));
                    }
                    json(ScopesBody { scopes })
                });
                respond(result);
            }
            command::SET_INSTRUCTION_BREAKPOINTS => {
                let result = args::<InstructionBreakpointsArgs>(arguments).and_then(|a| {
                    let mut state = hook.state.try_borrow_mut().map_err(|e| e.to_string())?;
                    json(state.set_points(&a.points))
                });
                respond(result);
            }
            command::SET_STEP_BREAKPOINTS => {
                let result = args::<StepBreakpointsArgs>(arguments).and_then(|a| {
                    let verified = a
                        .steps
                        .iter()
                        .map(|s| parse_step(stop.program, s).is_ok())
                        .collect();
                    let mut state = hook.state.try_borrow_mut().map_err(|e| e.to_string())?;
                    state.pending_steps = Some(a.steps);
                    json(crate::protocol::BreakpointsBody { verified })
                });
                respond(result);
            }
            command::CONTINUE => {
                respond(Ok(Value::Null));
                hook.conn.event(event::CONTINUED, Value::Null);
                return;
            }
            command::NEXT | command::STEP_IN | command::STEP_OUT => {
                if let Ok(mut state) = hook.state.try_borrow_mut() {
                    state.step_from(stop, &command);
                }
                respond(Ok(Value::Null));
                hook.conn.event(event::CONTINUED, Value::Null);
                return;
            }
            command::PAUSE => respond(Ok(Value::Null)),
            command::DISCONNECT => {
                if let Ok(mut state) = hook.state.try_borrow_mut() {
                    state.armed = false;
                    state.breakpoints.clear();
                    state.clear_step();
                    state.detached = true;
                }
                respond(Ok(Value::Null));
                return;
            }
            _ => respond(Err(
                "the story is stopped inside code: continue, or step on to the next step, first"
                    .to_owned(),
            )),
        }
    }
}
