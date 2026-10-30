//! `kagari/swap`: scene hot swap. The debug artifacts of the running and the rebuilt program
//! say which code changed (body hashes, story variable types); the engine moves the story
//! over ([`Engine::debug_swap`](kagari_engine::Engine::debug_swap)).

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use kagari_bytecode::{Constant, Instr, StaticKind};
use kagari_engine::debug::{StepId, SwapOutcome, SwapPlan};
use kagari_vm::{FnKind, FuncId, Program};

use super::{Debugger, Result, Target};
use crate::frames::Artifact;
use crate::protocol::SwapBody;

/// Which code of the running program changed in the rebuilt one.
pub(crate) struct Changes {
    /// Per old function: whether the code a frame of it runs changed (its body, or a
    /// non-scene function it reaches without entering a scene).
    changed: Vec<bool>,
    /// Per old function: its body hash.
    old_hashes: Vec<String>,
    /// The new program's body hashes by function id.
    new_hashes: Vec<String>,
    /// Why the story must restart from its entry (story variables changed).
    restart: Option<String>,
}

impl Changes {
    /// Compares `old` with `new` through their artifacts.
    pub(crate) fn of(
        old: &Program,
        old_artifact: &Artifact,
        new: &Program,
        new_artifact: &Artifact,
    ) -> Result<Changes> {
        let old_hashes = hashes(old, old_artifact)?;
        let new_hashes = hashes(new, new_artifact)?;
        let count = old_hashes.len();
        let own: Vec<bool> = (0..count)
            .map(|i| {
                let f = FuncId(index(i));
                let Some(n) = new.find_function(old.function_path(f)) else {
                    return true;
                };
                old.kind(f) != new.kind(n)
                    || old.params(f) != new.params(n)
                    || old_hashes.get(i) != new_hashes.get(n.0 as usize)
            })
            .collect();
        let refs = references(old);
        let is_scene = |f: FuncId| old.kind(f) == Some(FnKind::Scene);
        let changed = (0..count)
            .map(|i| {
                // Depth-first over what the frame runs: never into another scene.
                let mut seen = HashSet::from([i]);
                let mut todo = vec![i];
                while let Some(at) = todo.pop() {
                    if own.get(at).copied().unwrap_or(true) {
                        return true;
                    }
                    for next in refs.get(at).into_iter().flatten() {
                        let n = next.0 as usize;
                        if !is_scene(*next) && seen.insert(n) {
                            todo.push(n);
                        }
                    }
                }
                false
            })
            .collect();
        Ok(Changes {
            changed,
            old_hashes,
            new_hashes,
            restart: story_variables(old, old_artifact, new, new_artifact),
        })
    }

    pub(crate) fn changed(&self, f: FuncId) -> bool {
        self.changed.get(f.0 as usize).copied().unwrap_or(true)
    }

    pub(crate) fn same_code(&self, old: FuncId, new: FuncId) -> bool {
        match (
            self.old_hashes.get(old.0 as usize),
            self.new_hashes.get(new.0 as usize),
        ) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        }
    }
}

fn index(i: usize) -> u32 {
    u32::try_from(i).unwrap_or(u32::MAX)
}

/// The body hash of every function, by id; the artifact must be the program's.
fn hashes(program: &Program, artifact: &Artifact) -> Result<Vec<String>> {
    let functions = &artifact.debug.functions;
    if functions.len() != program.module().functions.len() {
        return Err("the debug artifact does not belong to this game.kagari".to_owned());
    }
    functions
        .iter()
        .enumerate()
        .map(|(i, f)| {
            if f.path == program.function_path(FuncId(index(i))) {
                Ok(f.body_hash.clone())
            } else {
                Err("the debug artifact does not belong to this game.kagari".to_owned())
            }
        })
        .collect()
}

/// Per function, the functions its code names: calls, function pointers, closures and
/// function constants.
fn references(program: &Program) -> Vec<Vec<FuncId>> {
    let module = program.module();
    // Per constant, the functions it holds (constants only refer to lower ones).
    let mut in_constant: Vec<Vec<FuncId>> = Vec::with_capacity(module.constants.len());
    for constant in &module.constants {
        let mut funcs = Vec::new();
        let mut children = |ids: &[kagari_bytecode::ConstId]| {
            for id in ids {
                if let Some(inner) = in_constant.get(id.0 as usize) {
                    funcs.extend(inner.iter().copied());
                }
            }
        };
        match constant {
            Constant::Function(f) => funcs.push(*f),
            Constant::Ruby { base, reading } => children(&[*base, *reading]),
            Constant::MonoRuby { base, readings } => {
                children(&[*base]);
                children(readings);
            }
            Constant::Effect { base, effect } => children(&[*base, *effect]),
            Constant::Concat(ids)
            | Constant::Tuple(ids)
            | Constant::Array(ids)
            | Constant::Vec(ids)
            | Constant::Adt { fields: ids, .. } => children(ids),
            _ => {}
        }
        funcs.sort_by_key(|f| f.0);
        funcs.dedup();
        in_constant.push(funcs);
    }
    module
        .functions
        .iter()
        .map(|function| {
            let mut out = Vec::new();
            for instr in &function.code {
                match instr {
                    Instr::Call { func, .. }
                    | Instr::Func { func, .. }
                    | Instr::Closure { func, .. } => out.push(*func),
                    Instr::Const { value, .. } => {
                        if let Some(funcs) = in_constant.get(value.0 as usize) {
                            out.extend(funcs.iter().copied());
                        }
                    }
                    _ => {}
                }
            }
            out
        })
        .collect()
}

/// Why the story variables no longer fit: one added, removed or of another type.
fn story_variables(
    old: &Program,
    old_artifact: &Artifact,
    new: &Program,
    new_artifact: &Artifact,
) -> Option<String> {
    let vars = |program: &Program, artifact: &Artifact| -> HashMap<String, String> {
        program
            .module()
            .statics
            .iter()
            .enumerate()
            .filter(|(_, s)| s.kind == StaticKind::StoryVariable)
            .map(|(i, s)| {
                let path = program.string(s.path).to_owned();
                let ty = artifact
                    .debug
                    .statics
                    .get(i)
                    .filter(|d| d.path == path)
                    .map(|d| d.ty.clone())
                    .unwrap_or_default();
                (path, ty)
            })
            .collect()
    };
    let (before, after) = (vars(old, old_artifact), vars(new, new_artifact));
    let mut paths: Vec<&String> = before.keys().chain(after.keys()).collect();
    paths.sort();
    paths.dedup();
    paths
        .into_iter()
        .find_map(|path| match (before.get(path), after.get(path)) {
            (None, Some(_)) => Some(format!("the story variable `{path}` was added")),
            (Some(_), None) => Some(format!("the story variable `{path}` was removed")),
            (Some(a), Some(b)) if a != b => Some(format!(
                "the story variable `{path}` changed type from `{a}` to `{b}`"
            )),
            _ => None,
        })
}

impl Debugger {
    /// Swaps `bytes` (with its debug artifact) in for the running program; see
    /// [`Engine::debug_swap`](kagari_engine::Engine::debug_swap). The story graph, the
    /// tracker and the instruction breakpoints start over; step breakpoints move by path.
    pub fn swap(
        &mut self,
        t: &mut Target<'_>,
        bytes: &[u8],
        artifact: Artifact,
    ) -> Result<SwapBody> {
        let old_artifact = self.artifact.clone().ok_or(
            "hot swap needs the debug artifact of the running game: build it with `--profile \
             debug`",
        )?;
        let old = t.engine.program().clone();
        let new = Program::load(bytes).map_err(|e| format!("the new game.kagari: {e}"))?;
        let changes = Changes::of(&old, &old_artifact, &new, &artifact)?;
        let changed = |f: FuncId| changes.changed(f);
        let same_code = |a: FuncId, b: FuncId| changes.same_code(a, b);
        let plan = SwapPlan {
            changed: &changed,
            same_code: &same_code,
            restart: changes.restart.clone(),
        };
        let breakpoints: Vec<(String, u32)> = self
            .step_breakpoints
            .iter()
            .map(|s| (old.function_path(s.func).to_owned(), s.id))
            .collect();
        let swapped = t
            .engine
            .debug_swap(bytes, t.platform, t.now_ms, &plan)
            .map_err(|e| format!("the new game.kagari does not start: {e}"))?;
        let program = t.engine.program().clone();
        self.artifact = Some(Rc::new(artifact));
        self.summaries = None;
        self.tracked = None;
        self.hook.borrow_mut().reprogram(program.clone());
        self.step_breakpoints = breakpoints
            .iter()
            .filter_map(|(path, id)| {
                Some(StepId {
                    func: program.find_function(path)?,
                    id: *id,
                })
            })
            .collect();
        t.engine
            .debug_set_breakpoints(self.step_breakpoints.iter().copied());
        let (outcome, scene, reason) = match swapped.outcome {
            SwapOutcome::NoGame => ("none", None, None),
            SwapOutcome::Kept => ("kept", None, None),
            SwapOutcome::Recalled { scene } => ("recalled", Some(scene), None),
            SwapOutcome::Restarted { reason } => ("restarted", None, Some(reason)),
            SwapOutcome::Reset { reason } => ("reset", None, Some(reason)),
        };
        Ok(SwapBody {
            outcome: outcome.to_owned(),
            scene,
            reason,
            thrown: swapped.thrown,
            position: self.position(t),
            script_hash: t.engine.script_hash().to_owned(),
        })
    }
}
