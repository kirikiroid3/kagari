//! Stacks, scopes and variables in DAP's form: lines and columns as the editor counts them, and
//! variable references for the parts of values.

use serde_json::{Value, json};

use crate::client::Locator;
use crate::protocol::{ScopesBody, StackBody, Var};

/// Where the editor's lines and columns start: 1 or 0 (`linesStartAt1`, `columnsStartAt1`).
pub(super) struct Lines {
    pub line_base: u32,
    pub column_base: u32,
}

impl Default for Lines {
    fn default() -> Lines {
        Lines {
            line_base: 1,
            column_base: 1,
        }
    }
}

/// A 1-based number counted from `base`.
fn rebase(n: u32, base: u32) -> u32 {
    n.saturating_sub(1).saturating_add(base)
}

impl Lines {
    /// The editor's line as a 1-based line.
    pub fn source_line(&self, line: u32) -> u32 {
        line.saturating_sub(self.line_base).saturating_add(1)
    }

    /// DAP `stackTrace` frames `start..start + levels` of `stack`.
    pub fn stack_frames(
        &self,
        locator: Option<&Locator>,
        stack: &StackBody,
        start: usize,
        levels: Option<usize>,
    ) -> Value {
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
                let located = match (locator, frame.file.as_deref(), frame.offset) {
                    (Some(locator), Some(file), Some(offset)) => locator.locate(file, offset),
                    _ => None,
                };
                match located {
                    Some(at) => json!({
                        "id": id,
                        "name": name,
                        "source": {
                            "name": at.path.file_name().map(|n| n.to_string_lossy().into_owned()),
                            "path": at.path,
                        },
                        "line": rebase(at.line, self.line_base),
                        "column": rebase(at.column, self.column_base),
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
}

/// `variablesReference` `n` is entry `n - 1`; cleared whenever the story moves.
#[derive(Default)]
pub(super) struct VarRefs(Vec<Vec<Var>>);

impl VarRefs {
    pub fn clear(&mut self) {
        self.0.clear();
    }

    fn reference(&mut self, vars: Vec<Var>) -> usize {
        self.0.push(vars);
        self.0.len()
    }

    /// DAP `scopes`, each with a reference to its variables.
    pub fn scopes(&mut self, body: ScopesBody) -> Value {
        let scopes: Vec<Value> = body
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
    }

    /// The DAP `variables` of `reference`; a variable with parts gets a fresh reference.
    pub fn variables(&mut self, reference: usize) -> Result<Vec<Value>, String> {
        let vars = reference
            .checked_sub(1)
            .and_then(|i| self.0.get(i))
            .cloned()
            .ok_or("the variables are gone: the story moved")?;
        Ok(vars
            .into_iter()
            .map(|var| {
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
            })
            .collect())
    }
}
