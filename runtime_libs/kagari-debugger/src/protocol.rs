//! The Kagari debug protocol (KDP): what a debugger front end (the `kagari debug` REPL, the
//! `kagari dap` adapter) and a runtime say to each other.
//!
//! The framing and the message shapes are DAP's base protocol (`Content-Length` headers, JSON
//! requests, responses and events), so the adapter reuses one codec for both sides. The
//! payloads are Kagari's: positions are step IDs and source locations are `file` + byte
//! `offset`, since a runtime has no sources; the adapter turns them into lines.

use std::io::{self, BufRead, Write};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A protocol message.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Message {
    Request {
        seq: i64,
        command: String,
        #[serde(default)]
        arguments: Value,
    },
    Response {
        seq: i64,
        request_seq: i64,
        success: bool,
        command: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        #[serde(default)]
        body: Value,
    },
    Event {
        seq: i64,
        event: String,
        #[serde(default)]
        body: Value,
    },
}

/// Reads one framed message body; `None` at the end of the stream.
pub fn read_frame(r: &mut impl BufRead) -> io::Result<Option<Vec<u8>>> {
    let mut length: Option<usize> = None;
    let mut line = String::new();
    loop {
        line.clear();
        if r.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let header = line.trim_end_matches(['\r', '\n']);
        if header.is_empty() {
            if length.is_some() {
                break;
            }
            continue;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            length = Some(value.trim().parse().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "a malformed Content-Length")
            })?);
        }
    }
    let mut body = vec![0; length.unwrap_or(0)];
    r.read_exact(&mut body)?;
    Ok(Some(body))
}

/// Writes one framed message body.
pub fn write_frame(w: &mut impl Write, body: &[u8]) -> io::Result<()> {
    write!(w, "Content-Length: {}\r\n\r\n", body.len())?;
    w.write_all(body)?;
    w.flush()
}

/// The requests a runtime serves.
pub mod command {
    pub const INITIALIZE: &str = "initialize";
    pub const DISCONNECT: &str = "disconnect";
    pub const SET_STEP_BREAKPOINTS: &str = "setStepBreakpoints";
    pub const SET_INSTRUCTION_BREAKPOINTS: &str = "setInstructionBreakpoints";
    pub const CONTINUE: &str = "continue";
    pub const NEXT: &str = "next";
    pub const STEP_IN: &str = "stepIn";
    pub const STEP_OUT: &str = "stepOut";
    pub const PAUSE: &str = "pause";
    pub const STACK_TRACE: &str = "stackTrace";
    pub const SCOPES: &str = "scopes";
    pub const STACK: &str = "kagari/stack";
    pub const LS: &str = "kagari/ls";
    pub const GOTO: &str = "kagari/goto";
    pub const PUSH: &str = "kagari/push";
    pub const POP: &str = "kagari/pop";
    pub const DETAIL: &str = "kagari/detail";
    pub const NAV: &str = "kagari/nav";
    pub const SEED: &str = "kagari/seed";
}

/// The events a runtime sends.
pub mod event {
    /// [`super::StoppedBody`]: the story stopped (and is held).
    pub const STOPPED: &str = "stopped";
    /// The story runs again (no body).
    pub const CONTINUED: &str = "continued";
    /// [`super::OutputBody`]: a message for the user.
    pub const OUTPUT: &str = "output";
}

/// The body of `initialize`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeBody {
    /// The script hash of the running `game.kagari`.
    pub script_hash: String,
    /// The debug artifact, when one matching the bytecode was loaded.
    pub artifact: Option<ArtifactInfo>,
    /// `true` for a runtime the player plays (a window), `false` for the headless query
    /// engine, where `continue` drives the story itself.
    pub live: bool,
    /// Whether the story is stopped now.
    pub stopped: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactInfo {
    /// Whether it has a story graph (`nav` needs one).
    pub graph: bool,
    /// Whether the graph is complete.
    pub complete: bool,
    /// Why not: the `W5006`/`W5007` reasons.
    pub incomplete: Vec<Incomplete>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Incomplete {
    pub code: String,
    pub message: String,
}

/// `setStepBreakpoints`: stop when the story enters one of these step IDs.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StepBreakpointsArgs {
    pub steps: Vec<String>,
}

/// `setInstructionBreakpoints`: stop before the instruction at a code word of a function.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InstructionBreakpointsArgs {
    pub points: Vec<InstructionPoint>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstructionPoint {
    /// The function's path in the bytecode.
    pub function: String,
    /// A code word index into the function's code, as in the bytecode's Debug section.
    pub word: u32,
}

/// One verdict per requested breakpoint, in order.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BreakpointsBody {
    pub verified: Vec<bool>,
}

/// A named value; aggregates have children.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Var {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Var>,
}

/// A frame of the story's call stack.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FrameInfo {
    /// The function's path (a scene's instance path).
    pub function: String,
    /// `scene`, `instant`, `const` or `fn`.
    pub kind: String,
    /// Its source file as the bytecode names it, and the byte offset of the frame's
    /// instruction (the step, or the call site of an outer frame).
    pub file: Option<String>,
    pub offset: Option<u32>,
    /// The step ID when the frame is at a step.
    pub step: Option<String>,
    /// The arguments' current values.
    pub args: Vec<Var>,
}

/// `kagari/stack`, `stackTrace`: innermost frame first.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StackBody {
    pub frames: Vec<FrameInfo>,
}

/// `scopes`: the frame's index in the stack (0 innermost).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ScopesArgs {
    pub frame: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Scope {
    pub name: String,
    pub vars: Vec<Var>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ScopesBody {
    pub scopes: Vec<Scope>,
}

/// `kagari/ls`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LsArgs {
    pub head: Option<u32>,
    pub tail: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LsLine {
    pub step: String,
    /// `say` or `choice`.
    pub kind: String,
    pub who: Option<String>,
    /// The plain text (a choice: its `id`).
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LsBody {
    /// The lines kept by `head`/`tail`, the current step first when kept.
    pub lines: Vec<LsLine>,
    /// How many lines there are before `head`/`tail`.
    pub total: usize,
    /// Why the listing ends: `choice`, `end` (the story returns) or `limit`.
    pub ends: String,
}

/// `kagari/goto`, `kagari/nav`'s target, `kagari/detail`'s optional step.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StepArgs {
    pub step: Option<String>,
}

/// Where the story is.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub step: Option<String>,
    /// Choices taken so far.
    pub choices: usize,
    /// Lines advanced since the last choice.
    pub steps: u32,
    pub seed: u64,
}

/// `kagari/push`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PushArgs {
    pub arms: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PushBody {
    pub position: Position,
    /// The choice ID of each selection made.
    pub selected: Vec<String>,
}

/// `kagari/detail`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Detail {
    pub step: String,
    /// `say` or `choice`.
    pub kind: String,
    /// The speaker and the text with their formatting (rubies, effects).
    pub who: Option<String>,
    pub text: Option<String>,
    pub voice: Option<String>,
    /// The render function's path.
    pub render: Option<String>,
    /// Whether the render is a keyframe; `None` without a story graph that knows it.
    pub keyframe: Option<bool>,
    /// A choice's `id`.
    pub choice: Option<String>,
    pub arms: Vec<ArmDetail>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ArmDetail {
    pub arm: String,
    pub text: String,
    pub shown: bool,
}

/// `kagari/nav`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NavArgs {
    pub step: String,
    /// The most seeds to try when the path needs draws.
    pub seeds: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NavBody {
    /// `reachable`, `unreachable` or `unknown`.
    pub answer: String,
    /// Why `unknown` (or `unreachable`).
    pub reason: Option<String>,
    /// The shortest path: its choices and draws in order.
    pub path: Vec<PathItem>,
    /// The `push` arguments that realize the path.
    pub push: Vec<String>,
    /// Steps the path passes, the target included.
    pub length: u32,
    /// The seed search, when the path needs draws.
    pub seed: Option<SeedSearch>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum PathItem {
    /// Select `arm` at the choice `step`.
    Choice { step: String, arm: String },
    /// The scene `scene` must draw `value` (one of `outcomes` values).
    Draw {
        scene: String,
        value: String,
        outcomes: u32,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SeedSearch {
    /// Draws on the path.
    pub draws: u32,
    /// One in how many seeds draws them all, if the draws were independent and uniform.
    pub odds: u64,
    /// Seeds tried (the current seed first, then 0, 1, …), and the most allowed.
    pub tried: u32,
    pub limit: u32,
    /// The current seed.
    pub current: u64,
    /// A seed whose replay of the current trace plus the path reaches the target.
    pub found: Option<u64>,
}

/// `kagari/seed`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SeedArgs {
    pub seed: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SeedBody {
    pub seed: u64,
}

/// The `stopped` event.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoppedBody {
    /// `entry`, `breakpoint`, `step`, `pause`, `choice` (a choice needs an arm), `end` (the
    /// story would return), `goto` (a move: goto, push, pop, seed) or `error`.
    pub reason: String,
    /// The step ID the story is at.
    pub step: Option<String>,
    pub description: Option<String>,
    /// Stopped inside code (an instruction breakpoint or stepping), not at a step.
    pub in_code: bool,
}

/// The `output` event.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OutputBody {
    pub text: String,
}
