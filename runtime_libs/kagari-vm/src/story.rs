//! The story coroutine: σ = frames + story statics + PRNG. A fiber runs its entry scene until
//! the next STEP, then waits for the step's result.
//!
//! A fiber may record **call snapshots** (debugging only, [`StoryFiber::record_calls`]): at
//! every scene call, what the called frame can change behind its caller (the story variables,
//! the values behind reference arguments, the PRNG). Unwinding to one
//! ([`StoryFiber::unwind_to_call`]) throws the frames off and leaves the caller on its call,
//! which the next resume runs again; scene hot swap re-calls a changed scene this way.

use std::any::Any;
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use kagari_bytecode::{FnKind, FuncId, Instr, StaticId, StaticKind};

use crate::exec::{self, Exit, Frame, StoryCtx};
use crate::hook::FrameView;
use crate::host::Host;
use crate::program::{Program, new_cell};
use crate::refs;
use crate::remap::{Remap, RemapError};
use crate::rng::Rng;
use crate::trap::{StepId, Trap, TrapKind, fault};
use crate::value::{Ref, Value};
use crate::vm::Vm;

/// One cell per story-variable static, from its init constant.
#[derive(Debug)]
pub struct StoryStatics(Box<[Option<Rc<RefCell<Value>>>]>);

impl StoryStatics {
    /// Every story variable at its initial value.
    pub fn fresh(program: &Program) -> StoryStatics {
        StoryStatics(
            program
                .module()
                .statics
                .iter()
                .map(|s| {
                    (s.kind == StaticKind::StoryVariable).then(|| {
                        new_cell(program.constant(s.init).cloned().unwrap_or(Value::Uninit))
                    })
                })
                .collect(),
        )
    }

    /// The cell of `var`; `None` for statics that are not story variables.
    pub fn cell(&self, var: StaticId) -> Option<&Rc<RefCell<Value>>> {
        self.0.get(var.0 as usize)?.as_ref()
    }

    /// The value of every story variable, by static index (`None` for other statics or a
    /// cell being written).
    fn values(&self) -> Box<[Option<Value>]> {
        self.0
            .iter()
            .map(|cell| {
                cell.as_ref()
                    .and_then(|c| c.try_borrow().ok())
                    .map(|v| v.clone())
            })
            .collect()
    }
}

/// What a scene call can change behind its caller, as it was when the call started.
struct CallSnapshot {
    /// The stack index of the called frame.
    depth: usize,
    /// Every story variable, by static index.
    statics: Box<[Option<Value>]>,
    /// The reference arguments and the values behind them.
    targets: Vec<(Ref, Value)>,
    rng: Rng,
    last_step: Option<StepId>,
    /// The host's mark of the resume that made the call ([`StoryFiber::set_call_mark`]).
    mark: Option<Rc<dyn Any>>,
    /// `false` when something could not be recorded (a reference argument into a host part):
    /// the call cannot be unwound to.
    complete: bool,
}

/// The call snapshots of the scene frames on the stack, bottom first, and the current mark.
#[derive(Default)]
pub(crate) struct CallLog {
    snapshots: Vec<CallSnapshot>,
    mark: Option<Rc<dyn Any>>,
}

impl fmt::Debug for CallLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CallLog")
            .field("snapshots", &self.snapshots.len())
            .finish()
    }
}

impl CallLog {
    /// The frame at `depth` was just pushed: snapshot what it can change when it is a scene.
    pub(crate) fn called(
        &mut self,
        program: &Program,
        frame: &Frame,
        depth: usize,
        statics: &StoryStatics,
        rng: &Rng,
        last_step: Option<StepId>,
    ) {
        if program.kind(frame.func) != Some(FnKind::Scene) {
            return;
        }
        let params = program.params(frame.func).map_or(0, usize::from);
        let mut complete = true;
        let mut targets = Vec::new();
        for arg in frame.regs.iter().take(params) {
            if let Value::Ref(r) = arg {
                match refs::read(r) {
                    Ok(value) => targets.push((r.clone(), value)),
                    Err(_) => complete = false,
                }
            }
        }
        self.snapshots.push(CallSnapshot {
            depth,
            statics: statics.values(),
            targets,
            rng: rng.clone(),
            last_step,
            mark: self.mark.clone(),
            complete,
        });
    }

    /// The stack is `len` frames high now: the snapshots of frames that returned go.
    pub(crate) fn returned(&mut self, len: usize) {
        while self.snapshots.last().is_some_and(|s| s.depth >= len) {
            self.snapshots.pop();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FiberState {
    /// Runs from the top frame's `pc` without an input: a new fiber, or one unwound to a call.
    Fresh,
    Suspended,
    Done,
    Failed,
}

/// The story coroutine.
#[derive(Debug)]
pub struct StoryFiber {
    stack: Vec<Frame>,
    statics: StoryStatics,
    rng: Rng,
    state: FiberState,
    /// The last step reached, named in the trap of a later panic.
    last_step: Option<StepId>,
    /// The values RAND produced during the last resume, in order.
    draws: Vec<Value>,
    /// The call snapshots, when recorded.
    calls: Option<CallLog>,
}

/// Why a resume stopped.
#[derive(Debug)]
pub enum Suspend {
    /// A STEP with its record and its step ID.
    Step { record: Value, step: StepId },
    /// The entry scene returned.
    Returned(Value),
}

/// Rebuilds a host's call mark for another program ([`StoryFiber::transplant`]).
pub type MarkRemap<'a> =
    dyn FnMut(&Rc<dyn Any>, &mut Remap<'_>) -> Result<Rc<dyn Any>, RemapError> + 'a;

impl StoryFiber {
    /// A fiber at the start of the entry scene `entry`; nothing runs yet.
    pub fn start(
        vm: &Vm,
        entry: FuncId,
        args: Vec<Value>,
        statics: StoryStatics,
        seed: u64,
    ) -> Result<StoryFiber, Trap> {
        let program = vm.program();
        let is_scene = program
            .function(entry)
            .is_some_and(|f| f.kind == FnKind::Scene);
        if !is_scene {
            return Err(Trap::new(TrapKind::KindMismatch));
        }
        let frame = exec::new_frame(program, entry, args)?;
        Ok(StoryFiber {
            stack: vec![frame],
            statics,
            rng: Rng::from_seed(seed),
            state: FiberState::Fresh,
            last_step: None,
            draws: Vec::new(),
            calls: None,
        })
    }

    /// Records a snapshot at every scene call from now on (debugging only: memory grows with
    /// the call depth).
    pub fn record_calls(&mut self) {
        self.calls.get_or_insert_with(CallLog::default);
    }

    /// Whether call snapshots are recorded.
    pub fn records_calls(&self) -> bool {
        self.calls.is_some()
    }

    /// The host's mark for the snapshots of the calls the next resumes make, until the next
    /// mark; ignored when no calls are recorded.
    pub fn set_call_mark(&mut self, mark: Rc<dyn Any>) {
        if let Some(calls) = self.calls.as_mut() {
            calls.mark = Some(mark);
        }
    }

    /// The current mark: the one of the resume that reached the current step.
    pub fn call_mark(&self) -> Option<&Rc<dyn Any>> {
        self.calls.as_ref()?.mark.as_ref()
    }

    /// Whether the call that pushed frame `depth` can be unwound to.
    pub fn has_call_snapshot(&self, depth: usize) -> bool {
        self.calls
            .as_ref()
            .is_some_and(|c| c.snapshots.iter().any(|s| s.depth == depth && s.complete))
    }

    /// Throws off frame `depth` and every frame above it, and restores what the call that
    /// pushed it snapshotted (story variables, the values behind reference arguments, the
    /// PRNG, the last step). The caller is left on its CALL: the next resume (with any input)
    /// runs the call again. Returns the call's mark.
    pub fn unwind_to_call(
        &mut self,
        program: &Program,
        depth: usize,
    ) -> Result<Option<Rc<dyn Any>>, String> {
        if !matches!(self.state, FiberState::Fresh | FiberState::Suspended) {
            return Err("the story is not running".to_owned());
        }
        let calls = self
            .calls
            .as_mut()
            .ok_or("no call snapshots are recorded")?;
        let index = calls
            .snapshots
            .iter()
            .position(|s| s.depth == depth)
            .ok_or("the call was not snapshotted")?;
        let snapshot = calls.snapshots.get(index).ok_or("no such snapshot")?;
        if !snapshot.complete {
            return Err("the call passed a reference into the engine".to_owned());
        }
        // The caller is still on its call: a caller's `pc` stays on the CALL while it runs.
        let on_call = depth
            .checked_sub(1)
            .and_then(|i| self.stack.get(i))
            .and_then(|caller| program.function(caller.func)?.code.get(caller.pc as usize))
            .is_some_and(|i| matches!(i, Instr::Call { .. } | Instr::CallVal { .. }));
        if !on_call || depth >= self.stack.len() {
            return Err("no caller on a call below that frame".to_owned());
        }
        for (cell, value) in self.statics.0.iter().zip(snapshot.statics.iter()) {
            if let (Some(cell), Some(value)) = (cell, value) {
                *cell
                    .try_borrow_mut()
                    .map_err(|_| "a story variable is borrowed".to_owned())? = value.clone();
            }
        }
        for (target, value) in &snapshot.targets {
            refs::with_mut(target, |v| *v = value.clone()).map_err(|t| t.to_string())?;
        }
        self.rng = snapshot.rng.clone();
        self.last_step = snapshot.last_step;
        let mark = snapshot.mark.clone();
        calls.snapshots.truncate(index);
        self.stack.truncate(depth);
        self.draws.clear();
        self.state = FiberState::Fresh;
        Ok(mark)
    }

    /// The record and step ID of the STEP a suspended fiber is at.
    pub fn step_record(&self, program: &Program) -> Option<(Value, StepId)> {
        if self.state != FiberState::Suspended {
            return None;
        }
        let top = self.stack.last()?;
        let Some(Instr::Step { record, id, .. }) =
            program.function(top.func)?.code.get(top.pc as usize)
        else {
            return None;
        };
        let value = match top.regs.get(usize::from(*record))? {
            Value::Ref(r) => refs::read(r).ok()?,
            Value::Uninit => return None,
            v => v.clone(),
        };
        Some((
            value,
            StepId {
                func: top.func,
                id: *id,
            },
        ))
    }

    /// This fiber rebuilt for another program (scene hot swap): every frame's function, the
    /// story variables and every value by path ([`Remap`]); the snapshots' marks through
    /// `mark`. Each frame's function must have the same code layout in the new program (the
    /// caller keeps only frames whose code is unchanged), so `pc` and registers stay valid.
    pub fn transplant(
        &self,
        remap: &mut Remap<'_>,
        mark: &mut MarkRemap<'_>,
    ) -> Result<StoryFiber, RemapError> {
        let old = remap.old();
        let new = remap.new_program();
        // Story variables by path; every new one must have been one before, and back.
        let story = |program: &Program| -> Vec<String> {
            program
                .module()
                .statics
                .iter()
                .filter(|s| s.kind == StaticKind::StoryVariable)
                .map(|s| program.string(s.path).to_owned())
                .collect()
        };
        let (old_vars, new_vars) = (story(old), story(new));
        if let Some(added) = new_vars.iter().find(|p| !old_vars.contains(p)) {
            return Err(RemapError::StoryVariable(format!(
                "the story variable `{added}` was added"
            )));
        }
        if let Some(removed) = old_vars.iter().find(|p| !new_vars.contains(p)) {
            return Err(RemapError::StoryVariable(format!(
                "the story variable `{removed}` was removed"
            )));
        }
        // Old static index → new static index.
        let static_map: Vec<Option<usize>> = old
            .module()
            .statics
            .iter()
            .map(|s| {
                remap
                    .static_by_path(old.string(s.path))
                    .map(|id| id.0 as usize)
            })
            .collect();
        let statics = StoryStatics::fresh(new);
        for (old_index, cell) in self.statics.0.iter().enumerate() {
            let Some(cell) = cell else { continue };
            let new_cell = static_map
                .get(old_index)
                .copied()
                .flatten()
                .and_then(|i| statics.0.get(i))
                .and_then(Option::as_ref)
                .ok_or_else(|| {
                    RemapError::StoryVariable("a story variable changed kind".to_owned())
                })?;
            remap.seed_cell(cell, new_cell.clone());
        }
        for (old_index, cell) in self.statics.0.iter().enumerate() {
            let (Some(cell), Some(Some(new_index))) = (cell, static_map.get(old_index)) else {
                continue;
            };
            let value = cell
                .try_borrow()
                .map_err(|_| RemapError::Host("borrowed cell".to_owned()))?
                .clone();
            let value = remap.value(&value)?;
            if let Some(Some(target)) = statics.0.get(*new_index) {
                *target
                    .try_borrow_mut()
                    .map_err(|_| RemapError::Host("borrowed cell".to_owned()))? = value;
            }
        }
        let mut stack = Vec::with_capacity(self.stack.len());
        for frame in &self.stack {
            let func = remap.func(frame.func)?;
            let path = old.function_path(frame.func);
            let lines_up = match (old.function(frame.func), new.function(func)) {
                (Some(a), Some(b)) => {
                    a.code.len() == b.code.len()
                        && a.regs == b.regs
                        && a.code
                            .iter()
                            .zip(&b.code)
                            .all(|(x, y)| x.opcode() == y.opcode())
                }
                _ => false,
            };
            if !lines_up {
                return Err(RemapError::Frame(path.to_owned()));
            }
            let regs = frame
                .regs
                .iter()
                .map(|v| remap.value(v))
                .collect::<Result<_, _>>()?;
            stack.push(Frame {
                func,
                pc: frame.pc,
                regs,
            });
        }
        let step = |remap: &mut Remap<'_>, step: Option<StepId>| {
            step.and_then(|s| {
                remap
                    .func(s.func)
                    .ok()
                    .map(|func| StepId { func, id: s.id })
            })
        };
        let calls = match &self.calls {
            None => None,
            Some(log) => {
                let mut snapshots = Vec::with_capacity(log.snapshots.len());
                for s in &log.snapshots {
                    let mut values: Vec<Option<Value>> = vec![None; new.module().statics.len()];
                    for (old_index, value) in s.statics.iter().enumerate() {
                        if let (Some(value), Some(Some(new_index))) =
                            (value, static_map.get(old_index))
                            && let Some(slot) = values.get_mut(*new_index)
                        {
                            *slot = Some(remap.value(value)?);
                        }
                    }
                    let mut targets = Vec::with_capacity(s.targets.len());
                    for (r, v) in &s.targets {
                        targets.push((remap.reference(r)?, remap.value(v)?));
                    }
                    snapshots.push(CallSnapshot {
                        depth: s.depth,
                        statics: values.into_boxed_slice(),
                        targets,
                        rng: s.rng.clone(),
                        last_step: step(remap, s.last_step),
                        mark: s.mark.as_ref().map(|m| mark(m, remap)).transpose()?,
                        complete: s.complete,
                    });
                }
                Some(CallLog {
                    snapshots,
                    mark: log.mark.as_ref().map(|m| mark(m, remap)).transpose()?,
                })
            }
        };
        Ok(StoryFiber {
            stack,
            statics,
            rng: self.rng.clone(),
            state: self.state,
            last_step: step(remap, self.last_step),
            draws: Vec::new(),
            calls,
        })
    }

    /// Runs until the next STEP or the entry scene's return. `input` is the previous STEP's
    /// result (`Value::Tuple(empty)` for the first resume and for Say).
    pub fn resume(
        &mut self,
        vm: &mut Vm,
        host: &mut dyn Host,
        input: Value,
    ) -> Result<Suspend, Trap> {
        self.draws.clear();
        match self.state {
            FiberState::Done | FiberState::Failed => return Err(Trap::new(TrapKind::Unreachable)),
            FiberState::Suspended => {
                let program = vm.program().clone();
                let top = self.stack.last_mut().ok_or_else(fault)?;
                let dst = match program
                    .function(top.func)
                    .and_then(|f| f.code.get(top.pc as usize))
                {
                    Some(Instr::Step { dst, .. }) => *dst,
                    _ => return Err(fault()),
                };
                *top.regs.get_mut(usize::from(dst)).ok_or_else(fault)? = input;
                top.pc = top.pc.checked_add(1).ok_or_else(fault)?;
            }
            FiberState::Fresh => {}
        }
        let frames = u32::try_from(self.stack.len()).unwrap_or(u32::MAX);
        let max = vm.program().module().options.max_call_depth;
        match vm.depth.checked_add(frames).filter(|depth| *depth <= max) {
            Some(depth) => vm.depth = depth,
            None => {
                self.state = FiberState::Failed;
                return Err(Trap::new(TrapKind::CallDepth));
            }
        }
        vm.enter();
        let result = exec::run(
            vm,
            host,
            &mut self.stack,
            Some(StoryCtx {
                statics: &self.statics,
                rng: &mut self.rng,
                draws: &mut self.draws,
                calls: self.calls.as_mut(),
                last_step: self.last_step,
            }),
        );
        vm.leave();
        match result {
            Ok(Exit::Step(record, id)) => {
                let frames = u32::try_from(self.stack.len()).unwrap_or(u32::MAX);
                vm.depth = vm.depth.saturating_sub(frames);
                self.state = FiberState::Suspended;
                let top = self.stack.last().ok_or_else(fault)?;
                let step = StepId { func: top.func, id };
                self.last_step = Some(step);
                Ok(Suspend::Step { record, step })
            }
            Ok(Exit::Return(value)) => {
                self.state = FiberState::Done;
                Ok(Suspend::Returned(value))
            }
            Err(mut trap) => {
                self.state = FiberState::Failed;
                trap.step = self.last_step;
                Err(trap)
            }
        }
    }

    pub fn statics(&self) -> &StoryStatics {
        &self.statics
    }

    /// The fiber's frames, bottom (the entry scene) first; empty once it returned or failed.
    /// A suspended fiber's top frame is on its STEP, every other frame on its CALL.
    pub fn frames(&self) -> Vec<FrameView<'_>> {
        match self.state {
            FiberState::Done | FiberState::Failed => Vec::new(),
            FiberState::Fresh | FiberState::Suspended => {
                self.stack.iter().map(Frame::view).collect()
            }
        }
    }

    /// The values RAND produced during the last `resume`, in order (cleared when a resume
    /// starts).
    pub fn draws(&self) -> &[Value] {
        &self.draws
    }
}
