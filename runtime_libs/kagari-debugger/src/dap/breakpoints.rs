//! Line breakpoints: what each requested line of each file breaks at, and the union of all of
//! them as the runtime's step and instruction breakpoints. After a hot swap every line is
//! resolved again against the new sources and `game.kagari`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::answers::Waiting;
use super::editor::Args;
use super::frames::Lines;
use super::{Adapter, Session};
use crate::client::LineBreakpoints;
use crate::protocol::{InstructionPoint, command};

/// A requested line: its DAP breakpoint ID, the line as the editor counts it, and what it
/// breaks at.
struct Line {
    id: i64,
    line: u32,
    at: LineBreakpoints,
}

/// The requested lines of a file, under the path the editor named it by.
struct File {
    path: PathBuf,
    lines: Vec<Line>,
}

/// The requested lines of each source file, with what each breaks at.
#[derive(Default)]
pub(super) struct Breakpoints {
    files: HashMap<PathBuf, File>,
    next_id: i64,
}

impl Breakpoints {
    /// Replaces the breakpoints of `path`: `found` pairs each requested line (as the editor
    /// counts it) with what it breaks at. Returns the DAP breakpoints.
    fn set(&mut self, path: &Path, found: Vec<(u32, LineBreakpoints)>) -> Vec<Value> {
        let lines: Vec<Line> = found
            .into_iter()
            .map(|(line, at)| {
                self.next_id = self.next_id.saturating_add(1);
                Line {
                    id: self.next_id,
                    line,
                    at,
                }
            })
            .collect();
        let answer = lines
            .iter()
            .map(|l| breakpoint(l.id, l.line, path, &l.at))
            .collect();
        let key = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if lines.is_empty() {
            self.files.remove(&key);
        } else {
            let path = path.to_path_buf();
            self.files.insert(key, File { path, lines });
        }
        answer
    }

    /// Resolves every requested line again; returns the DAP breakpoints.
    fn resolve_again(&mut self, session: &Session, lines: &Lines) -> Vec<Value> {
        let mut changed = Vec::new();
        for file in self.files.values_mut() {
            for l in &mut file.lines {
                l.at = resolve(Some(session), lines, &file.path, l.line);
                changed.push(breakpoint(l.id, l.line, &file.path, &l.at));
            }
        }
        changed
    }

    /// Every file's step IDs and instruction points, each once.
    fn union(&self) -> (Vec<String>, Vec<InstructionPoint>) {
        let mut seen_steps = HashSet::new();
        let mut seen_points = HashSet::new();
        let mut steps = Vec::new();
        let mut points = Vec::new();
        for line in self.files.values().flat_map(|f| &f.lines) {
            let at = &line.at;
            steps.extend(at.steps.iter().filter(|s| seen_steps.insert(*s)).cloned());
            points.extend(
                at.points
                    .iter()
                    .filter(|p| seen_points.insert((&p.function, p.word)))
                    .cloned(),
            );
        }
        (steps, points)
    }
}

/// What the editor's `line` of `path` breaks at.
fn resolve(session: Option<&Session>, lines: &Lines, path: &Path, line: u32) -> LineBreakpoints {
    match session {
        Some(session) => session
            .locator
            .breakpoints_at(path, lines.source_line(line)),
        None => LineBreakpoints {
            reason: Some("no game is being debugged".to_owned()),
            ..LineBreakpoints::default()
        },
    }
}

/// A DAP breakpoint for the requested `line` (as the editor counts it).
pub(super) fn breakpoint(id: i64, line: u32, path: &Path, at: &LineBreakpoints) -> Value {
    let verified = !at.steps.is_empty() || !at.points.is_empty();
    let mut out = json!({
        "id": id,
        "verified": verified,
        "line": line,
        "source": { "path": path },
    });
    if !verified && let Some(object) = out.as_object_mut() {
        let reason = at.reason.as_deref().unwrap_or("nothing to stop at");
        object.insert("message".to_owned(), Value::from(reason));
    }
    out
}

impl Adapter<'_> {
    /// `setBreakpoints`: answered at once from the sources; the runtime gets the union of every
    /// file's breakpoints.
    pub(super) fn set_breakpoints(&mut self, a: Args) -> Result<Value, String> {
        let path = PathBuf::from(
            a.source
                .path
                .ok_or("`setBreakpoints` needs a source with a path")?,
        );
        let lines: Vec<u32> = if a.breakpoints.is_empty() {
            a.lines
        } else {
            a.breakpoints
                .iter()
                .filter_map(|b| b.get("line")?.as_u64()?.try_into().ok())
                .collect()
        };
        let found = lines
            .into_iter()
            .map(|line| {
                let at = resolve(self.session.as_ref(), &self.lines, &path, line);
                (line, at)
            })
            .collect();
        let answer = self.breakpoints.set(&path, found);
        if self.session.is_some() {
            self.send_breakpoints()?;
        }
        Ok(json!({ "breakpoints": answer }))
    }

    /// After a hot swap: every requested line is resolved again against the new sources and
    /// `game.kagari`, the editor gets a `breakpoint` event for each, and the runtime the union.
    pub(super) fn reset_breakpoints(&mut self) -> Result<(), String> {
        let session = self
            .session
            .as_ref()
            .ok_or("no game is being debugged: launch or attach first")?;
        for changed in self.breakpoints.resolve_again(session, &self.lines) {
            self.editor.event(
                "breakpoint",
                json!({ "reason": "changed", "breakpoint": changed }),
            );
        }
        self.send_breakpoints()
    }

    /// Sends the union of every file's breakpoints to the runtime.
    fn send_breakpoints(&mut self) -> Result<(), String> {
        let (steps, points) = self.breakpoints.union();
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
}
