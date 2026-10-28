//! The story coroutine: σ = frames + story statics + PRNG. A fiber runs its entry scene until
//! the next STEP, then waits for the step's result.

use std::cell::RefCell;
use std::rc::Rc;

use kagari_bytecode::{FnKind, FuncId, Instr, StaticId, StaticKind};

use crate::exec::{self, Exit, Frame, StoryCtx};
use crate::host::Host;
use crate::program::{Program, new_cell};
use crate::rng::Rng;
use crate::trap::{Trap, TrapKind, fault};
use crate::value::Value;
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FiberState {
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
}

/// Why a resume stopped.
#[derive(Debug)]
pub enum Suspend {
    /// A STEP with its record; `ordinal` is the index of this STEP among the STEPs of `func`,
    /// in code order.
    Step {
        record: Value,
        func: FuncId,
        ordinal: u32,
    },
    /// The entry scene returned.
    Returned(Value),
}

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
            }),
        );
        vm.leave();
        match result {
            Ok(Exit::Step(record)) => {
                let frames = u32::try_from(self.stack.len()).unwrap_or(u32::MAX);
                vm.depth = vm.depth.saturating_sub(frames);
                self.state = FiberState::Suspended;
                let top = self.stack.last().ok_or_else(fault)?;
                let ordinal = vm.program().step_ordinal(top.func, top.pc);
                match ordinal {
                    Some(ordinal) => Ok(Suspend::Step {
                        record,
                        func: top.func,
                        ordinal,
                    }),
                    None => {
                        self.state = FiberState::Failed;
                        Err(fault())
                    }
                }
            }
            Ok(Exit::Return(value)) => {
                self.state = FiberState::Done;
                Ok(Suspend::Returned(value))
            }
            Err(trap) => {
                self.state = FiberState::Failed;
                Err(trap)
            }
        }
    }

    pub fn statics(&self) -> &StoryStatics {
        &self.statics
    }
}
