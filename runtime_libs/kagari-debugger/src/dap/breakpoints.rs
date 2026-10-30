//! Line breakpoints: what each requested line of each file breaks at, and the union of all of
//! them as the runtime's step and instruction breakpoints.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::Adapter;
use super::answers::Waiting;
use super::editor::Args;
use crate::client::LineBreakpoints;
use crate::protocol::{InstructionPoint, command};

/// The requested lines of each source file, with what each breaks at.
#[derive(Default)]
pub(super) struct Breakpoints {
    files: HashMap<PathBuf, Vec<LineBreakpoints>>,
    next_id: i64,
}

impl Breakpoints {
    /// Replaces the breakpoints of `path`: `found` pairs each requested line (as the editor
    /// counts it) with what it breaks at. Returns the DAP breakpoints.
    fn set(&mut self, path: &Path, found: Vec<(u32, LineBreakpoints)>) -> Vec<Value> {
        let answer = found
            .iter()
            .map(|(line, at)| {
                self.next_id = self.next_id.saturating_add(1);
                breakpoint(self.next_id, *line, path, at)
            })
            .collect();
        let key = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let lines: Vec<LineBreakpoints> = found.into_iter().map(|(_, at)| at).collect();
        if lines.is_empty() {
            self.files.remove(&key);
        } else {
            self.files.insert(key, lines);
        }
        answer
    }

    /// Every file's step IDs and instruction points, each once.
    fn union(&self) -> (Vec<String>, Vec<InstructionPoint>) {
        let mut seen_steps = HashSet::new();
        let mut seen_points = HashSet::new();
        let mut steps = Vec::new();
        let mut points = Vec::new();
        for at in self.files.values().flatten() {
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
                let at = match &self.session {
                    Some(session) => session
                        .locator
                        .breakpoints_at(&path, self.lines.source_line(line)),
                    None => LineBreakpoints {
                        reason: Some("no game is being debugged".to_owned()),
                        ..LineBreakpoints::default()
                    },
                };
                (line, at)
            })
            .collect();
        let answer = self.breakpoints.set(&path, found);
        if self.session.is_some() {
            let (steps, points) = self.breakpoints.union();
            let waiting = || Waiting::Breakpoints;
            self.forward(
                command::SET_STEP_BREAKPOINTS,
                json!({ "steps": steps }),
                waiting(),
            )?;
            self.forward(
                command::SET_INSTRUCTION_BREAKPOINTS,
                json!({ "points": points }),
                waiting(),
            )?;
        }
        Ok(json!({ "breakpoints": answer }))
    }
}
