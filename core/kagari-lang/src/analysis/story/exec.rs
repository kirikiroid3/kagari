//! Running a scene's graph from one suspension to the next: every instruction in between is
//! instant code, run by the const-evaluation interpreter in story mode.

use rustc_hash::FxHashMap;

use crate::eval::ConstValue;
use crate::eval::interp::{
    BodyRef, Ctx, ErrorKind, EvalError, Exit, Frame, GenericEnv, Machine, Place, Value,
    subst_fn_ref,
};
use crate::hir::ExprId;
use crate::item_tree::ItemId;
use crate::ty::FnRef;

use super::cfg::{BlockId, Callee, Instr, Slot, SlotKind, StoryBody, Term, WriteMode};

/// CFG blocks one segment may pass before it is given up: every loop of a scene passes a
/// step (checked before exploration), so only a bug can exhaust this.
const SEGMENT_FUEL: u32 = 1_000_000;

/// The largest `for` loop the story may run (its items are materialized).
const MAX_FOR_ITEMS: i128 = 1 << 20;

/// Where a segment stopped.
pub(super) enum Stop {
    Step {
        block: BlockId,
        record: ConstValue,
    },
    Call {
        block: BlockId,
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

/// The state a segment stops in: the kept slots of the scene's frame and every story
/// variable.
pub(super) struct Snapshot {
    pub frame: Vec<Option<ConstValue>>,
    pub globals: Vec<ConstValue>,
    /// The last expression of the scene the segment ran: where it stopped.
    pub last: Option<ExprId>,
}

pub(super) struct Scene<'a, 'db> {
    pub b: BodyRef<'db>,
    pub body: &'a StoryBody,
}

/// Runs `scene` from the start of `block` with the given frame and story variables.
pub(super) fn run(
    ctx: &Ctx<'_>,
    scene: &Scene<'_, '_>,
    env: GenericEnv,
    block: BlockId,
    frame: &[Option<ConstValue>],
    vars: &[ItemId],
    globals: &[ConstValue],
) -> (Stop, Snapshot) {
    let mut machine = Machine::new(ctx);
    let mut story = FxHashMap::default();
    for (item, value) in vars.iter().zip(globals) {
        let value = machine.thaw(value);
        story.insert(*item, machine.alloc(value));
    }
    machine.story = Some(story);
    let mut segment = Segment {
        machine,
        frame: Frame::new(scene.b, env),
        body: scene.body,
        temps: vec![None; scene.body.slots.len()],
        pointees: FxHashMap::default(),
        last: None,
    };
    segment.load(frame);
    let stop = segment.run(block);
    let snapshot = segment.snapshot(&stop, vars);
    (stop, snapshot)
}

struct Segment<'a, 'c, 'db> {
    machine: Machine<'c, 'db>,
    frame: Frame<'db>,
    body: &'a StoryBody,
    /// The cells of the temporaries.
    temps: Vec<Option<usize>>,
    /// The cells holding the referents of reference parameters, by slot.
    pointees: FxHashMap<Slot, usize>,
    /// The last expression of the scene started.
    last: Option<ExprId>,
}

/// How an instruction leaves its block early.
enum Jump {
    To(BlockId),
    Stop(Stop),
}

// A `Stop` ends the segment: it is produced once per segment, so its size does not matter.
#[allow(clippy::result_large_err)]
impl Segment<'_, '_, '_> {
    fn load(&mut self, frame: &[Option<ConstValue>]) {
        for (slot, kind) in self.body.slots.iter().enumerate() {
            let value = frame.get(slot).and_then(Option::as_ref);
            match *kind {
                SlotKind::Local {
                    local,
                    by_ref: None,
                } => {
                    let value = value.map_or(Value::Uninit, |value| self.machine.thaw(value));
                    let cell = self.machine.alloc(value);
                    self.frame.locals.insert(local, cell);
                }
                SlotKind::Local {
                    local,
                    by_ref: Some(_),
                } => {
                    if let Some(value) = value {
                        let value = self.machine.thaw(value);
                        let pointee = self.machine.alloc(value);
                        let cell = self.machine.alloc(Value::Ref(Box::new(Place {
                            cell: pointee,
                            path: Vec::new(),
                            obj: None,
                        })));
                        self.frame.locals.insert(local, cell);
                        self.pointees.insert(slot, pointee);
                    }
                }
                SlotKind::Temp => {
                    if let Some(value) = value {
                        let value = self.machine.thaw(value);
                        let cell = self.machine.alloc(value);
                        if let Some(temp) = self.temps.get_mut(slot) {
                            *temp = Some(cell);
                        }
                    }
                }
            }
        }
    }

    fn cell(&self, slot: Slot) -> Option<usize> {
        match self.body.slots.get(slot)? {
            SlotKind::Local {
                by_ref: Some(_), ..
            } => self.pointees.get(&slot).copied(),
            SlotKind::Local { local, .. } => self.frame.locals.get(local).copied(),
            SlotKind::Temp => self.temps.get(slot).copied().flatten(),
        }
    }

    fn read(&mut self, slot: Slot) -> Result<Value, Stop> {
        let cell = self.cell(slot).ok_or(Stop::Abort)?;
        self.machine
            .read_place(&Place {
                cell,
                path: Vec::new(),
                obj: None,
            })
            .map_err(|_| Stop::Abort)
    }

    fn write(&mut self, slot: Slot, value: Value) {
        let cell = self.machine.alloc(value);
        match self.body.slots.get(slot) {
            Some(SlotKind::Local {
                by_ref: Some(_), ..
            }) => {
                self.pointees.insert(slot, cell);
            }
            Some(SlotKind::Local { local, .. }) => {
                self.frame.locals.insert(*local, cell);
            }
            Some(SlotKind::Temp) => {
                if let Some(temp) = self.temps.get_mut(slot) {
                    *temp = Some(cell);
                }
            }
            None => {}
        }
    }

    fn freeze(&mut self, value: &Value) -> Result<ConstValue, Stop> {
        self.machine.freeze(value).map_err(|_| Stop::Abort)
    }

    fn run(&mut self, start: BlockId) -> Stop {
        let mut block = start;
        let mut fuel = SEGMENT_FUEL;
        'blocks: loop {
            fuel = match fuel.checked_sub(1) {
                Some(fuel) => fuel,
                None => return Stop::Abort,
            };
            let Some(data) = self.body.blocks.get(block) else {
                return Stop::Abort;
            };
            for instr in &data.instrs {
                match self.instr(instr) {
                    Ok(()) => {}
                    Err(Jump::To(next)) => {
                        block = next;
                        continue 'blocks;
                    }
                    Err(Jump::Stop(stop)) => return stop,
                }
            }
            match self.term(block, &data.term) {
                Ok(next) => block = next,
                Err(stop) => return stop,
            }
        }
    }

    /// Where an interpreter exit other than a value goes.
    fn exit(&mut self, at: ExprId, exit: Exit) -> Jump {
        match exit {
            Exit::Break(target, value) => match self.body.loop_exits.get(&target).copied() {
                Some(loop_exit) => {
                    if let Some(dest) = loop_exit.dest {
                        self.write(dest, value);
                    }
                    Jump::To(loop_exit.brk)
                }
                None => Jump::Stop(Stop::Abort),
            },
            Exit::Continue(target) => match self.body.loop_exits.get(&target) {
                Some(loop_exit) => Jump::To(loop_exit.cont),
                None => Jump::Stop(Stop::Abort),
            },
            Exit::Return(value) => {
                self.write(self.body.ret, value);
                Jump::To(self.body.ret_block)
            }
            Exit::Error(error) => Jump::Stop(fault(error, at)),
        }
    }

    fn eval(&mut self, expr: ExprId) -> Result<Value, Jump> {
        match self.machine.eval(&mut self.frame, expr) {
            Ok(value) => Ok(value),
            Err(exit) => Err(self.exit(expr, exit)),
        }
    }

    /// Makes the operands evaluated before `expr` visible to it.
    fn set_overrides(&mut self, expr: ExprId, overrides: &[(ExprId, Slot)]) -> Result<(), Jump> {
        self.last = Some(expr);
        // In a loop, an operand evaluated on an earlier lap is evaluated again.
        self.frame.overrides.remove(&expr);
        for (operand, slot) in overrides {
            let cell = self.cell(*slot).ok_or(Jump::Stop(Stop::Abort))?;
            self.frame.overrides.insert(*operand, cell);
        }
        Ok(())
    }

    /// The place `expr` denotes, dereferenced `derefs` times.
    fn place(&mut self, expr: ExprId, derefs: u32) -> Result<Place, Jump> {
        let mut place = match self.machine.eval_place(&mut self.frame, expr) {
            Ok(place) => place,
            Err(exit) => return Err(self.exit(expr, exit)),
        };
        for _ in 0..derefs {
            place = match self.machine.deref_place(&self.frame, expr, &place) {
                Ok(place) => place,
                Err(exit) => return Err(self.exit(expr, exit)),
            };
        }
        Ok(place)
    }

    fn instr(&mut self, instr: &Instr) -> Result<(), Jump> {
        match instr {
            Instr::Eval {
                expr,
                dest,
                overrides,
            } => {
                self.set_overrides(*expr, overrides)?;
                let value = self.eval(*expr)?;
                if let Some(dest) = dest {
                    self.write(*dest, value);
                }
            }
            Instr::EvalDeref {
                expr,
                dest,
                overrides,
            } => {
                self.set_overrides(*expr, overrides)?;
                let value = self.eval(*expr)?;
                let value = self
                    .machine
                    .deref_value(value)
                    .map_err(|kind| Jump::Stop(fault_kind(kind, *expr)))?;
                self.write(*dest, value);
            }
            Instr::Receiver {
                expr,
                dest,
                derefs,
                by_place,
                overrides,
            } => {
                self.set_overrides(*expr, overrides)?;
                let value = if *by_place {
                    let place = self.place(*expr, *derefs)?;
                    self.machine
                        .read_place(&place)
                        .map_err(|kind| Jump::Stop(fault_kind(kind, *expr)))?
                } else {
                    let mut value = self.eval(*expr)?;
                    for _ in 0..*derefs {
                        let place = self
                            .machine
                            .ref_place(&value)
                            .map_err(|kind| Jump::Stop(fault_kind(kind, *expr)))?;
                        value = self
                            .machine
                            .read_place(&place)
                            .map_err(|kind| Jump::Stop(fault_kind(kind, *expr)))?;
                    }
                    value
                };
                self.write(*dest, value);
            }
            Instr::Bind { pat, src } => {
                let value = self.read(*src).map_err(Jump::Stop)?;
                if let Err(exit) = self.machine.bind(&mut self.frame, *pat, value) {
                    return Err(Jump::Stop(exit_stop(exit)));
                }
            }
            Instr::Declare { pat } => self.machine.declare(&mut self.frame, *pat),
            Instr::Const { dest, value } => {
                let value = self.machine.thaw(value);
                self.write(*dest, value);
            }
            Instr::ForInit { slot } => {
                let value = self.read(*slot).map_err(Jump::Stop)?;
                let items = self.items(value).map_err(Jump::Stop)?;
                self.write(*slot, Value::Array(items));
            }
            Instr::WriteBack {
                expr,
                src,
                mode,
                overrides,
            } => {
                self.set_overrides(*expr, overrides)?;
                let value = self.read(*src).map_err(Jump::Stop)?;
                let place = match mode {
                    WriteMode::Place { derefs } => self.place(*expr, *derefs)?,
                    WriteMode::ThroughRef => {
                        let reference = self.eval(*expr)?;
                        self.machine
                            .ref_place(&reference)
                            .map_err(|kind| Jump::Stop(fault_kind(kind, *expr)))?
                    }
                };
                self.machine
                    .write_place(&place, value)
                    .map_err(|kind| Jump::Stop(fault_kind(kind, *expr)))?;
            }
        }
        Ok(())
    }

    /// The items a `for` loop runs over.
    fn items(&mut self, value: Value) -> Result<Vec<Value>, Stop> {
        let value = self.machine.deref_value(value).map_err(|_| Stop::Abort)?;
        let range = &self.machine.ctx.std.range;
        match value {
            Value::Adt { item, fields, .. } if item == range.item => {
                let field = |name: &str| range.field(name).and_then(|index| fields.get(index));
                let (Some(Value::Int(start)), Some(Value::Int(end)), Some(Value::Bool(inclusive))) =
                    (field("start"), field("end"), field("inclusive"))
                else {
                    return Err(Stop::Abort);
                };
                let end = if *inclusive {
                    end.checked_add(1).ok_or(Stop::Abort)?
                } else {
                    *end
                };
                if end
                    .checked_sub(*start)
                    .is_none_or(|len| len > MAX_FOR_ITEMS)
                {
                    return Err(Stop::Abort);
                }
                Ok((*start..end).map(Value::Int).collect())
            }
            Value::Array(items) | Value::Vec(items) => Ok(items),
            _ => Err(Stop::Abort),
        }
    }

    fn term(&mut self, block: BlockId, term: &Term) -> Result<BlockId, Stop> {
        match term {
            Term::Goto(next) => Ok(*next),
            Term::If { cond, then, other } => match self.read(*cond)? {
                Value::Bool(true) => Ok(*then),
                Value::Bool(false) => Ok(*other),
                _ => Err(Stop::Abort),
            },
            Term::Test {
                pat,
                src,
                then,
                other,
            } => {
                let value = self.read(*src)?;
                match self.machine.test_pat(&mut self.frame, *pat, value) {
                    Ok(true) => Ok(*then),
                    Ok(false) => Ok(*other),
                    Err(exit) => Err(exit_stop(exit)),
                }
            }
            Term::ForNext {
                iter,
                pat,
                body,
                exit,
            } => {
                let Value::Array(mut items) = self.read(*iter)? else {
                    return Err(Stop::Abort);
                };
                if items.is_empty() {
                    return Ok(*exit);
                }
                let item = items.remove(0);
                self.write(*iter, Value::Array(items));
                match self.machine.bind(&mut self.frame, *pat, item) {
                    Ok(()) => Ok(*body),
                    Err(exit) => Err(exit_stop(exit)),
                }
            }
            Term::Step { expr, arg, .. } => {
                self.last = Some(*expr);
                let value = self.read(*arg)?;
                let record = self.freeze(&value)?;
                Err(Stop::Step { block, record })
            }
            Term::Call {
                expr, callee, args, ..
            } => {
                self.last = Some(*expr);
                let callee = match callee {
                    Callee::Direct(fn_ref) => subst_fn_ref(fn_ref, &self.frame.env),
                    Callee::Value { slot, .. } => match self.read(*slot)? {
                        Value::Fn(fn_ref) => fn_ref,
                        _ => return Err(Stop::Abort),
                    },
                };
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    let value = self.read(*arg)?;
                    values.push(self.freeze(&value)?);
                }
                Err(Stop::Call {
                    block,
                    callee,
                    args: values,
                })
            }
            Term::Return { value } => {
                let value = self.read(*value)?;
                let value = self.freeze(&value)?;
                let mut outs = Vec::with_capacity(self.body.out_params.len());
                for slot in &self.body.out_params {
                    let out = self.read(*slot)?;
                    outs.push(self.freeze(&out)?);
                }
                Err(Stop::Return { value, outs })
            }
            Term::Unreachable => Err(Stop::Abort),
        }
    }

    /// The state at `stop`: the slots kept at a suspension, and the story variables.
    fn snapshot(&mut self, stop: &Stop, vars: &[ItemId]) -> Snapshot {
        let mut frame = vec![None; self.body.slots.len()];
        let block = match stop {
            Stop::Step { block, .. } | Stop::Call { block, .. } => Some(*block),
            _ => None,
        };
        if let Some(kept) = block.and_then(|block| self.body.kept.get(&block)) {
            for (slot, value) in frame.iter_mut().enumerate() {
                if !kept.contains(slot) {
                    continue;
                }
                *value = self
                    .read(slot)
                    .ok()
                    .and_then(|read| self.machine.freeze(&read).ok());
            }
        }
        let mut globals = Vec::with_capacity(vars.len());
        for item in vars {
            let value = self
                .machine
                .story
                .as_ref()
                .and_then(|story| story.get(item))
                .copied()
                .and_then(|cell| self.machine.heap.get(cell).cloned())
                .and_then(|value| self.machine.freeze(&value).ok())
                .unwrap_or_else(ConstValue::unit);
            globals.push(value);
        }
        Snapshot {
            frame,
            globals,
            last: self.last,
        }
    }
}

fn fault(error: EvalError, at: ExprId) -> Stop {
    match error.kind {
        ErrorKind::Fault(_) => Stop::Fault {
            error,
            at: Some(at),
        },
        ErrorKind::Abort | ErrorKind::Inspected => Stop::Abort,
    }
}

fn fault_kind(kind: ErrorKind, at: ExprId) -> Stop {
    fault(EvalError { kind, span: None }, at)
}

fn exit_stop(exit: Exit) -> Stop {
    match exit {
        Exit::Error(EvalError {
            kind: ErrorKind::Fault(fault),
            span,
        }) => Stop::Fault {
            error: EvalError {
                kind: ErrorKind::Fault(fault),
                span,
            },
            at: None,
        },
        _ => Stop::Abort,
    }
}
