//! Frames and their variables for the protocol, from VM frames and the debug information.

use std::path::Path;

use kagari_bytecode::{FnKind, StaticKind};
use kagari_debug::{DebugArtifact, DebugInfo, FunctionDebug, StoryGraph};
use kagari_vm::{FrameView, StaticId, StoryStatics, Value};

use crate::protocol::{FrameInfo, Scope};
use crate::values::{Names, var};

/// What a debug build adds: the story graph and the debug information.
pub struct Artifact {
    pub graph: Option<StoryGraph>,
    pub debug: DebugInfo,
}

impl Artifact {
    /// The debug artifact next to `game.kagari` in `dir`, when there is one for this script
    /// hash. A file for another build is ignored with a warning.
    pub fn load(dir: &Path, script_hash: &str) -> Option<Artifact> {
        let path = dir.join(kagari_debug::DEBUG_FILE);
        let bytes = std::fs::read(&path).ok()?;
        match DebugArtifact::from_bytes(bytes) {
            Ok(artifact) if artifact.script_hash == script_hash => Some(Artifact {
                graph: artifact.story,
                debug: artifact.debug,
            }),
            Ok(_) => {
                log::warn!(
                    target: "kagari::debug",
                    "{} belongs to another build of game.kagari; ignored",
                    path.display()
                );
                None
            }
            Err(error) => {
                log::warn!(target: "kagari::debug", "{}: {error}; ignored", path.display());
                None
            }
        }
    }

    fn function(&self, names: Names<'_>, frame: &FrameView<'_>) -> Option<&FunctionDebug> {
        let debug = self.debug.functions.get(frame.func.0 as usize)?;
        // The table is parallel to the function table; check it is the same program.
        (debug.path == names.function(frame.func)).then_some(debug)
    }
}

pub fn kind_name(kind: Option<FnKind>) -> &'static str {
    match kind {
        Some(FnKind::Scene) => "scene",
        Some(FnKind::Instant) => "instant",
        Some(FnKind::Const) => "const",
        Some(FnKind::Fn) | None => "fn",
    }
}

/// A frame's named locals that hold a register at its instruction: `(name, is a parameter,
/// value)`. Without debug information, the parameters by position.
fn locals(
    artifact: Option<&Artifact>,
    names: Names<'_>,
    frame: &FrameView<'_>,
) -> Vec<(String, bool, Value)> {
    let params = usize::from(names.program.params(frame.func).unwrap_or(0));
    let word = names.program.word_of_instr(frame.func, frame.pc);
    if let (Some(debug), Some(word)) = (artifact.and_then(|a| a.function(names, frame)), word) {
        let mut out = Vec::new();
        for (index, local) in debug.locals.iter().enumerate() {
            let live = local
                .ranges
                .iter()
                .find(|r| r.start <= word && word < r.end);
            let Some(range) = live else {
                continue;
            };
            let Some(value) = frame.regs.get(range.register as usize) else {
                continue;
            };
            // Parameters are declared first.
            out.push((local.name.clone(), index < params, value.clone()));
        }
        return out;
    }
    frame
        .regs
        .iter()
        .take(params)
        .enumerate()
        .map(|(i, v)| (format!("arg{i}"), true, v.clone()))
        .collect()
}

/// A frame for `stackTrace`: `step` is the step ID when the frame is at a step.
pub fn frame_info(
    artifact: Option<&Artifact>,
    names: Names<'_>,
    frame: &FrameView<'_>,
    step: Option<String>,
) -> FrameInfo {
    let args = locals(artifact, names, frame)
        .into_iter()
        .filter(|(_, param, _)| *param)
        .map(|(name, _, value)| var(names, name, &value))
        .collect();
    FrameInfo {
        function: names.function(frame.func).to_owned(),
        kind: kind_name(names.program.kind(frame.func)).to_owned(),
        file: names.program.file(frame.func).map(str::to_owned),
        offset: names.program.offset(frame.func, frame.pc),
        step,
        args,
    }
}

/// The frame's `Arguments` and `Locals` scopes.
pub fn frame_scopes(
    artifact: Option<&Artifact>,
    names: Names<'_>,
    frame: &FrameView<'_>,
) -> Vec<Scope> {
    let mut args = Vec::new();
    let mut locals_out = Vec::new();
    for (name, param, value) in locals(artifact, names, frame) {
        if matches!(value, Value::Uninit) {
            continue;
        }
        let v = var(names, name, &value);
        if param {
            args.push(v);
        } else {
            locals_out.push(v);
        }
    }
    vec![
        Scope {
            name: "Arguments".to_owned(),
            vars: args,
        },
        Scope {
            name: "Locals".to_owned(),
            vars: locals_out,
        },
    ]
}

/// The story variables (`static mut`) by path.
pub fn story_vars(names: Names<'_>, statics: &StoryStatics) -> Scope {
    let module = names.program.module();
    let vars = module
        .statics
        .iter()
        .enumerate()
        .filter(|(_, s)| s.kind == StaticKind::StoryVariable)
        .filter_map(|(index, s)| {
            let id = StaticId(u32::try_from(index).ok()?);
            let value = statics.cell(id)?.try_borrow().ok()?.clone();
            Some(var(names, names.program.string(s.path), &value))
        })
        .collect();
    Scope {
        name: "Story variables".to_owned(),
        vars,
    }
}
