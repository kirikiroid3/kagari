//! Running a scene's MIR from one suspension to the next: every statement in between is
//! instant code, run by the MIR executor in story mode.
//!
//! A scene call copies its reference arguments in: the callee gets the values, each
//! reference parameter bound to a fresh cell holding it. On return the final values of the
//! callee's `&mut` parameters come back as the exit's `outs`, and the caller writes them
//! back through the places its arguments borrowed.
//!
//! Story variables are thawed when the segment first reaches them; only those are frozen
//! again at the stop ([`Snapshot::globals`]).

use rustc_hash::FxHashMap;

use crate::db::Db;
use crate::eval::ConstValue;
use crate::eval::machine::exec::Frame;
use crate::eval::machine::story::{StoryValues, StoryVars};
use crate::eval::machine::value::{GenericEnv, Value, subst_fn_ref, subst_ty};
use crate::eval::machine::{ErrorKind, EvalError, Machine, Place};
use crate::eval::std_items::StdTable;
use crate::hir::ExprId;
use crate::input::Program;
use crate::item_tree::ItemId;
use crate::mir::{BasicBlock, Body, Callee, Local, LocalSet, Terminator};
use crate::ty::{FnRef, Ty};

use super::liveness::{ref_arg, ref_params};

/// Blocks one segment may enter before it is given up: every loop of a scene passes a
/// step (checked before exploration), so only a bug can exhaust this.
const SEGMENT_FUEL: u32 = 1_000_000;

/// Where a segment stopped.
pub(super) enum Stop {
    Step {
        block: BasicBlock,
        record: ConstValue,
    },
    /// `rng_choose::<ty>()`: the story draws a value of `ty`.
    Rng { block: BasicBlock, ty: Ty },
    Call {
        block: BasicBlock,
        callee: FnRef,
        args: Vec<ConstValue>,
    },
    Return {
        value: ConstValue,
        outs: Vec<ConstValue>,
    },
    /// A const-evaluation error in story code, at the expression `at` of the scene (at
    /// the error's own location when `None`).
    Fault {
        error: EvalError,
        at: Option<ExprId>,
    },
    /// Evaluation could not continue without an error of its own (an unsupported value,
    /// such as a capturing closure held across a step).
    Abort,
}

/// The state a segment stops in: the kept locals of the scene's frame (in ascending order,
/// `None` when uninitialized) and the story variables the segment accessed.
pub(super) struct Snapshot {
    pub frame: Vec<Option<ConstValue>>,
    /// The position and final value of every story variable the segment accessed; the
    /// others kept their values.
    pub globals: Vec<(usize, ConstValue)>,
    /// The last expression of the scene the segment ran: where it stopped.
    pub last: Option<ExprId>,
}

/// A scene's body and what it keeps at each suspension.
#[derive(Clone, Copy)]
pub(super) struct SceneBody<'db> {
    pub body: &'db Body,
    pub kept: &'db FxHashMap<BasicBlock, LocalSet>,
}

/// Where a segment starts.
pub(super) enum Resume<'a> {
    /// At the scene's entry, with its arguments.
    Entry(&'a [ConstValue]),
    /// After the suspension ending `block`, whose kept locals hold `frame`, with the value
    /// the suspension produced and (for a scene call) the callee's `outs`.
    After {
        block: BasicBlock,
        frame: &'a [Option<&'a ConstValue>],
        value: &'a ConstValue,
        outs: &'a [ConstValue],
    },
}

/// The program-wide inputs of a segment.
#[derive(Clone, Copy)]
pub(super) struct Inputs<'a, 'db> {
    pub db: &'db dyn Db,
    pub program: Program,
    pub std: &'db StdTable,
    /// The position of each story variable.
    pub slots: &'a FxHashMap<ItemId, usize>,
}

/// Runs `scene` from `resume` with the story variables `globals`.
pub(super) fn run<'a, 'db: 'a>(
    inputs: Inputs<'a, 'db>,
    scene: SceneBody<'db>,
    env: GenericEnv,
    resume: Resume<'_>,
    globals: &'a dyn StoryValues,
) -> (Stop, Snapshot) {
    let mut machine: Machine<'a> = Machine::new(inputs.db, inputs.program, inputs.std);
    machine.story = Some(StoryVars::new(inputs.slots, globals));
    let body = scene.body;
    let cells = body
        .locals
        .iter()
        .map(|_| machine.alloc(Value::Uninit))
        .collect();
    let mut segment = Segment {
        machine,
        frame: Frame {
            body,
            env,
            cells,
            block: body.entry,
            stmt: 0,
        },
        scene,
        by_ref: ref_params(body).map(|(local, _)| local).collect(),
        last: None,
    };
    let stop = match segment.start(resume) {
        Ok(()) => segment.run(),
        Err(stop) => stop,
    };
    segment.snapshot(stop)
}

struct Segment<'db> {
    machine: Machine<'db>,
    frame: Frame<'db>,
    scene: SceneBody<'db>,
    /// The parameters passed by reference: kept as the value they point to.
    by_ref: Vec<Local>,
    last: Option<ExprId>,
}

impl<'db> Segment<'db> {
    fn cell(&self, local: Local) -> Option<usize> {
        self.frame
            .cells
            .get(local.into_raw().into_u32() as usize)
            .copied()
    }

    /// Stores a kept value into `local`; a reference parameter gets a fresh cell for the
    /// value and a reference to it.
    fn load(&mut self, local: Local, value: &ConstValue) {
        let value = self.machine.thaw(value);
        let mut cell = self.machine.alloc(value);
        if self.by_ref.contains(&local) {
            cell = self.machine.alloc(Value::Ref(Box::new(Place::cell(cell))));
        }
        if let Some(slot) = self
            .frame
            .cells
            .get_mut(local.into_raw().into_u32() as usize)
        {
            *slot = cell;
        }
    }

    /// The frozen value of `local`, through the reference of a reference parameter;
    /// `Ok(None)` when it is uninitialized.
    fn freeze_local(&mut self, local: Local) -> Result<Option<ConstValue>, ()> {
        let cell = self.cell(local).ok_or(())?;
        let value = self.machine.heap.get(cell).cloned().ok_or(())?;
        if matches!(value, Value::Uninit) {
            return Ok(None);
        }
        let value = if self.by_ref.contains(&local) {
            self.machine.deref_value(value).map_err(|_| ())?
        } else {
            value
        };
        self.machine.freeze(&value).map(Some).map_err(|_| ())
    }

    fn start(&mut self, resume: Resume<'_>) -> Result<(), Stop> {
        let body = self.scene.body;
        match resume {
            Resume::Entry(args) => {
                for (local, arg) in body.args().zip(args) {
                    self.load(local, arg);
                }
                Ok(())
            }
            Resume::After {
                block,
                frame,
                value,
                outs,
            } => {
                let kept = self.scene.kept.get(&block).ok_or(Stop::Abort)?;
                for (local, value) in kept.iter().zip(frame) {
                    if let Some(value) = value {
                        self.load(local, value);
                    }
                }
                let data = body.block(block).ok_or(Stop::Abort)?;
                self.last = data.term_src;
                let (dest, next) = match &data.term {
                    Terminator::Step { dest, next, .. }
                    | Terminator::Rng { dest, next, .. }
                    | Terminator::SceneCall { dest, next, .. } => (dest, *next),
                    _ => return Err(Stop::Abort),
                };
                let value = self.machine.thaw(value);
                self.write(dest, value, data.term_src)?;
                if let Terminator::SceneCall { args, .. } = &data.term {
                    let targets = args
                        .iter()
                        .filter_map(|arg| ref_arg(body, arg))
                        .filter(|target| target.mutable);
                    for (target, out) in targets.zip(outs) {
                        let out = self.machine.thaw(out);
                        self.write(&target.place, out, data.term_src)?;
                    }
                }
                self.frame.block = next;
                self.frame.stmt = 0;
                Ok(())
            }
        }
    }

    fn write(
        &mut self,
        place: &crate::mir::Place,
        value: Value,
        src: Option<ExprId>,
    ) -> Result<(), Stop> {
        let place = self
            .machine
            .scene_place(&self.frame, place, src)
            .map_err(|error| fault(error, src))?;
        self.machine
            .write_place(&place, value)
            .map_err(|kind| fault(EvalError { kind, at: None }, src))
    }

    fn freeze(&mut self, value: &Value) -> Result<ConstValue, Stop> {
        self.machine.freeze(value).map_err(|_| Stop::Abort)
    }

    fn run(&mut self) -> Stop {
        let mut fuel = SEGMENT_FUEL.saturating_sub(1);
        match self
            .machine
            .run_scene(&mut self.frame, &mut fuel, &mut self.last)
        {
            Ok(true) => {}
            Ok(false) => return Stop::Abort,
            Err(error) => return fault(error, self.last),
        }
        match self.suspend() {
            Ok(stop) | Err(stop) => stop,
        }
    }

    /// The stop at the terminator the scene reached.
    fn suspend(&mut self) -> Result<Stop, Stop> {
        let body = self.scene.body;
        let block = self.frame.block;
        let data = body.block(block).ok_or(Stop::Abort)?;
        let src = data.term_src;
        match &data.term {
            Terminator::Step { arg, .. } => {
                let value = self
                    .machine
                    .scene_operand(&self.frame, arg, src)
                    .map_err(|error| fault(error, src))?;
                let record = self.freeze(&value)?;
                Ok(Stop::Step { block, record })
            }
            Terminator::Rng { ty, .. } => Ok(Stop::Rng {
                block,
                ty: subst_ty(ty, &self.frame.env),
            }),
            Terminator::SceneCall { callee, args, .. } => {
                let callee = match callee {
                    Callee::Fn(fn_ref) => subst_fn_ref(fn_ref, &self.frame.env),
                    Callee::Value(operand) => {
                        let value = self
                            .machine
                            .scene_operand(&self.frame, operand, src)
                            .map_err(|error| fault(error, src))?;
                        match self.machine.deref_value(value) {
                            Ok(Value::Fn(fn_ref)) => fn_ref,
                            _ => return Err(Stop::Abort),
                        }
                    }
                    Callee::Builtin(..) => return Err(Stop::Abort),
                };
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    let mut value = self
                        .machine
                        .scene_operand(&self.frame, arg, src)
                        .map_err(|error| fault(error, src))?;
                    if ref_arg(body, arg).is_some() {
                        value = self
                            .machine
                            .deref_value(value)
                            .map_err(|kind| fault(EvalError { kind, at: None }, src))?;
                    }
                    values.push(self.freeze(&value)?);
                }
                Ok(Stop::Call {
                    block,
                    callee,
                    args: values,
                })
            }
            Terminator::Return => {
                let value = self
                    .freeze_local(Body::RETURN)
                    .ok()
                    .flatten()
                    .ok_or(Stop::Abort)?;
                let mut outs = Vec::new();
                for (local, mutable) in ref_params(body) {
                    if mutable {
                        let out = self.freeze_local(local).ok().flatten().ok_or(Stop::Abort)?;
                        outs.push(out);
                    }
                }
                Ok(Stop::Return { value, outs })
            }
            _ => Err(Stop::Abort),
        }
    }

    /// The state at `stop`: the locals kept at a suspension, and the story variables the
    /// segment accessed. A value the analysis cannot store turns the stop into an abort.
    fn snapshot(mut self, stop: Stop) -> (Stop, Snapshot) {
        let block = match &stop {
            Stop::Step { block, .. } | Stop::Rng { block, .. } | Stop::Call { block, .. } => {
                Some(*block)
            }
            _ => None,
        };
        let mut failed = false;
        let mut frame = Vec::new();
        if let Some(kept) = block.and_then(|block| self.scene.kept.get(&block)) {
            for local in kept.iter() {
                match self.freeze_local(local) {
                    Ok(value) => frame.push(value),
                    Err(()) => {
                        failed = true;
                        frame.push(None);
                    }
                }
            }
        }
        let accessed: Vec<(usize, usize)> = self
            .machine
            .story
            .as_ref()
            .map(|story| story.accessed().collect())
            .unwrap_or_default();
        let mut globals = Vec::with_capacity(accessed.len());
        for (slot, cell) in accessed {
            let value = self
                .machine
                .heap
                .get(cell)
                .cloned()
                .and_then(|value| self.machine.freeze(&value).ok());
            match value {
                Some(value) => globals.push((slot, value)),
                None => failed = true,
            }
        }
        let stop = match stop {
            Stop::Fault { .. } | Stop::Abort => stop,
            _ if failed => Stop::Abort,
            stop => stop,
        };
        let snapshot = Snapshot {
            frame,
            globals,
            last: self.last,
        };
        (stop, snapshot)
    }
}

fn fault(error: EvalError, at: Option<ExprId>) -> Stop {
    match error.kind {
        ErrorKind::Fault(_) => Stop::Fault { error, at },
        ErrorKind::Abort | ErrorKind::Inspected => Stop::Abort,
    }
}
