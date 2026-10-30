//! The interpreter loop: frames, control flow and calls. Data instructions are in `data.rs`.

mod data;

use std::borrow::Cow;

use kagari_bytecode::{FnKind, FuncId, Instr, RandDomain, Reg};

use crate::hook::{FrameView, HookStop};
use crate::host::{Host, HostFn};
use crate::int;
use crate::ops;
use crate::program::Program;
use crate::refs;
use crate::rng::Rng;
use crate::story::StoryStatics;
use crate::trap::{Trap, TrapKind, fault, mismatch};
use crate::value::{Adt, AdtTy, Ref, RefRoot, Value};
use crate::vm::Vm;

/// A call frame. `pc` is the index of the current instruction; it advances only once an
/// instruction completes, so a caller's `pc` stays on its CALL while the callee runs.
#[derive(Debug)]
pub(crate) struct Frame {
    pub func: FuncId,
    pub pc: u32,
    pub regs: Box<[Value]>,
}

impl Frame {
    pub fn view(&self) -> FrameView<'_> {
        FrameView {
            func: self.func,
            pc: self.pc,
            regs: &self.regs,
        }
    }
}

/// What only a story run has: the story statics, the PRNG and the log of its draws.
pub(crate) struct StoryCtx<'a> {
    pub statics: &'a StoryStatics,
    pub rng: &'a mut Rng,
    /// Every value RAND produced, in order.
    pub draws: &'a mut Vec<Value>,
}

/// How a run stopped.
pub(crate) enum Exit {
    /// The bottom frame returned.
    Return(Value),
    /// A STEP suspended the run with its record and step id; the top frame's `pc` is still
    /// on it.
    Step(Value, u32),
}

/// What a call value resolves to.
pub(crate) enum Callee {
    /// A frame of a function with these arguments.
    Frame(FuncId, Vec<Value>),
    /// A host function.
    Host(HostFn, Vec<Value>),
}

/// Runs the top frame of `stack` until the bottom frame returns or a STEP suspends. On a trap,
/// every frame is unwound into the backtrace (top first) and `stack` is left empty.
pub(crate) fn run(
    vm: &mut Vm,
    host: &mut dyn Host,
    stack: &mut Vec<Frame>,
    mut story: Option<StoryCtx<'_>>,
) -> Result<Exit, Trap> {
    let program = vm.program().clone();
    loop {
        // The only per-instruction cost of the hook while none is installed.
        if vm.hook.is_some() {
            check_hook(vm, stack, story.as_ref(), &program);
        }
        match step(vm, host, stack, &mut story, &program) {
            Ok(None) => {}
            Ok(Some(exit)) => return Ok(exit),
            Err(mut trap) => {
                for frame in stack.iter().rev() {
                    trap.backtrace.push((frame.func, frame.pc));
                    vm.depth = vm.depth.saturating_sub(1);
                }
                stack.clear();
                return Err(trap);
            }
        }
    }
}

/// Asks the installed hook whether to stop before the top frame's instruction, and lets it stop.
/// The hook is taken out of the VM while it stops, so it cannot be re-entered.
#[cold]
#[inline(never)]
fn check_hook(vm: &mut Vm, stack: &[Frame], story: Option<&StoryCtx<'_>>, program: &Program) {
    let (Some(top), Some(hook)) = (stack.last(), vm.hook.as_mut()) else {
        return;
    };
    if !hook.check(top.func, top.pc, vm.depth) {
        return;
    }
    let Some(mut hook) = vm.hook.take() else {
        return;
    };
    let frames: Vec<FrameView<'_>> = stack.iter().map(Frame::view).collect();
    hook.stop(&HookStop {
        program,
        types: vm.types(),
        frames: &frames,
        statics: story.map(|s| s.statics),
        depth: vm.depth,
    });
    vm.hook = Some(hook);
}

/// A frame of `func` with its parameters set from `args`; `KindMismatch` when the count
/// differs from the function's parameters.
pub(crate) fn new_frame(program: &Program, func: FuncId, args: Vec<Value>) -> Result<Frame, Trap> {
    let f = program
        .function(func)
        .ok_or(Trap::new(TrapKind::KindMismatch))?;
    if args.len() != usize::from(f.params) {
        return Err(Trap::new(TrapKind::KindMismatch));
    }
    let mut regs = args;
    regs.resize(usize::from(f.regs).max(regs.len()), Value::Uninit);
    Ok(Frame {
        func,
        pc: 0,
        regs: regs.into_boxed_slice(),
    })
}

/// Pushes a frame, counting it against the call depth.
pub(crate) fn push_frame(
    vm: &mut Vm,
    stack: &mut Vec<Frame>,
    func: FuncId,
    args: Vec<Value>,
) -> Result<(), Trap> {
    let max = vm.program().module().options.max_call_depth;
    if vm.depth >= max {
        return Err(Trap::new(TrapKind::CallDepth));
    }
    let frame = new_frame(vm.program(), func, args)?;
    vm.depth = vm
        .depth
        .checked_add(1)
        .ok_or(Trap::new(TrapKind::CallDepth))?;
    stack.push(frame);
    Ok(())
}

/// CALLVAL's rules on `callee` (already dereferenced) with the expected kind.
pub(crate) fn callee(
    program: &Program,
    callee: Value,
    kind: FnKind,
    args: Vec<Value>,
) -> Result<Callee, Trap> {
    let kind_mismatch = || Trap::new(TrapKind::KindMismatch);
    match callee {
        Value::Func(f) => {
            let function = program.function(f).ok_or_else(kind_mismatch)?;
            if function.kind != kind || usize::from(function.params) != args.len() {
                return Err(kind_mismatch());
            }
            Ok(Callee::Frame(f, args))
        }
        Value::Closure(c) => {
            let function = program.function(c.func).ok_or_else(kind_mismatch)?;
            let total = c.captures.len().checked_add(args.len());
            if function.kind != kind || total != Some(usize::from(function.params)) {
                return Err(kind_mismatch());
            }
            let mut all = c.captures.clone();
            all.extend(args);
            Ok(Callee::Frame(c.func, all))
        }
        Value::HostFn(h) => {
            if kind != FnKind::Fn || args.len() != usize::from(h.params) {
                return Err(kind_mismatch());
            }
            Ok(Callee::Host(h, args))
        }
        other => Err(mismatch("Func, Closure or HostFn", other.tag())),
    }
}

fn reg(regs: &[Value], r: Reg) -> Result<&Value, Trap> {
    regs.get(usize::from(r)).ok_or_else(fault)
}

fn set(regs: &mut [Value], r: Reg, value: Value) -> Result<(), Trap> {
    *regs.get_mut(usize::from(r)).ok_or_else(fault)? = value;
    Ok(())
}

/// A value operand: a reference is read through, `Uninit` panics.
fn val(regs: &[Value], r: Reg) -> Result<Cow<'_, Value>, Trap> {
    match reg(regs, r)? {
        Value::Ref(r) => refs::read(r).map(Cow::Owned),
        Value::Uninit => Err(Trap::new(TrapKind::ReadUninit)),
        v => Ok(Cow::Borrowed(v)),
    }
}

/// The raw registers `first..first + count`.
fn raw_range(regs: &[Value], first: Reg, count: u16) -> Result<Vec<Value>, Trap> {
    let start = usize::from(first);
    let end = start.checked_add(usize::from(count)).ok_or_else(fault)?;
    regs.get(start..end)
        .map(<[Value]>::to_vec)
        .ok_or_else(fault)
}

fn advance(frame: &mut Frame) -> Result<(), Trap> {
    frame.pc = frame.pc.checked_add(1).ok_or_else(fault)?;
    Ok(())
}

/// Executes the top frame's current instruction.
fn step(
    vm: &mut Vm,
    host: &mut dyn Host,
    stack: &mut Vec<Frame>,
    story: &mut Option<StoryCtx<'_>>,
    program: &Program,
) -> Result<Option<Exit>, Trap> {
    let frame = stack.last_mut().ok_or_else(fault)?;
    let instr = *program
        .function(frame.func)
        .and_then(|f| f.code.get(frame.pc as usize))
        .ok_or_else(fault)?;
    vm.fuel = vm
        .fuel
        .checked_sub(1)
        .ok_or(Trap::new(TrapKind::FuelExhausted))?;
    match instr {
        Instr::Jmp { target } => frame.pc = target.0,
        Instr::JmpIf { cond, target } | Instr::JmpNot { cond, target } => {
            let jump_if = matches!(instr, Instr::JmpIf { .. });
            if ops::boolean(&*val(&frame.regs, cond)?)? == jump_if {
                frame.pc = target.0;
            } else {
                advance(frame)?;
            }
        }
        Instr::Ret { src } => {
            let value = reg(&frame.regs, src)?.clone();
            if matches!(value, Value::Uninit) {
                return Err(Trap::new(TrapKind::ReadUninit));
            }
            stack.pop();
            vm.depth = vm.depth.saturating_sub(1);
            let Some(caller) = stack.last_mut() else {
                return Ok(Some(Exit::Return(value)));
            };
            let dst = match program
                .function(caller.func)
                .and_then(|f| f.code.get(caller.pc as usize))
            {
                Some(Instr::Call { dst, .. } | Instr::CallVal { dst, .. }) => *dst,
                _ => return Err(fault()),
            };
            set(&mut caller.regs, dst, value)?;
            advance(caller)?;
        }
        Instr::Call {
            args, argc, func, ..
        } => {
            let args = raw_range(&frame.regs, args, argc)?;
            push_frame(vm, stack, func, args)?;
        }
        Instr::CallImp {
            dst,
            args,
            argc,
            import,
        } => {
            let args = raw_range(&frame.regs, args, argc)?;
            let value = host.import(vm, import, args)?;
            let frame = stack.last_mut().ok_or_else(fault)?;
            set(&mut frame.regs, dst, value)?;
            advance(frame)?;
        }
        Instr::CallVal {
            dst,
            args,
            argc,
            callee: target,
            kind,
        } => {
            let target = val(&frame.regs, target)?.into_owned();
            let args = raw_range(&frame.regs, args, argc)?;
            match callee(program, target, kind, args)? {
                Callee::Frame(func, args) => push_frame(vm, stack, func, args)?,
                Callee::Host(f, args) => {
                    let value = host.call_host_fn(vm, &f, args)?;
                    let frame = stack.last_mut().ok_or_else(fault)?;
                    set(&mut frame.regs, dst, value)?;
                    advance(frame)?;
                }
            }
        }
        Instr::Step { record, id, .. } => {
            if story.is_none() {
                return Err(Trap::new(TrapKind::KindMismatch));
            }
            let record = val(&frame.regs, record)?.into_owned();
            return Ok(Some(Exit::Step(record, id)));
        }
        Instr::Rand { dst, domain } => {
            let Some(story) = story.as_mut() else {
                return Err(Trap::new(TrapKind::KindMismatch));
            };
            let value = rand(program, story.rng, domain)?;
            story.draws.push(value.clone());
            set(&mut frame.regs, dst, value)?;
            advance(frame)?;
        }
        Instr::RefStatic { dst, var } => {
            let Some(story) = story.as_ref() else {
                return Err(Trap::new(TrapKind::KindMismatch));
            };
            let cell = story.statics.cell(var).ok_or_else(fault)?.clone();
            let r = Ref {
                root: RefRoot::Cell(cell),
                path: program.empty_path.clone(),
            };
            set(&mut frame.regs, dst, Value::Ref(r))?;
            advance(frame)?;
        }
        other => {
            data::exec(other, frame, program, &mut vm.fuel)?;
            advance(frame)?;
        }
    }
    Ok(None)
}

/// RAND: a uniform variant of a fieldless enum, or a uniform value of an int type.
fn rand(program: &Program, rng: &mut Rng, domain: RandDomain) -> Result<Value, Trap> {
    let module = program.module();
    match domain {
        RandDomain::Enum(ty) => {
            let variants = module
                .types
                .get(ty.0 as usize)
                .ok_or_else(fault)?
                .variants
                .len();
            let n = u128::try_from(variants).map_err(|_| fault())?;
            let variant = rng.uniform(n).ok_or_else(fault)?;
            Ok(Value::Adt(std::rc::Rc::new(Adt {
                ty: AdtTy::Module(ty),
                variant: u32::try_from(variant).map_err(|_| fault())?,
                fields: Vec::new(),
            })))
        }
        RandDomain::Int(t) => {
            let it = module.int_types.get(t.0 as usize).ok_or_else(fault)?;
            let overflow = || Trap::new(TrapKind::Overflow);
            let span = (it.hi.wrapping_sub(it.lo) as u128)
                .checked_add(1)
                .filter(|span| *span <= 1u128 << 64)
                .ok_or_else(overflow)?;
            let r = rng.uniform(span).ok_or_else(fault)?;
            let value = it.lo.checked_add(i128::from(r)).ok_or_else(overflow)?;
            Ok(Value::Int(value))
        }
    }
}

/// Both operands of an integer instruction.
fn int_operands(regs: &[Value], a: Reg, b: Reg) -> Result<(i128, i128), Trap> {
    int::operands(&*val(regs, a)?, &*val(regs, b)?)
}
