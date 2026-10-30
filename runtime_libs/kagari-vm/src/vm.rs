//! The VM: a program, the type table, and the call depth and fuel shared by nested runs.

use std::fmt;
use std::rc::Rc;

use kagari_bytecode::{FnKind, FuncId};

use crate::FUEL_PER_RUN;
use crate::exec::{self, Callee, Exit};
use crate::hook::InstrHook;
use crate::host::Host;
use crate::program::Program;
use crate::refs;
use crate::trap::{Trap, TrapKind};
use crate::types::TypeTable;
use crate::value::{Ref, Value};

/// Runs a program's functions. Runs nest: a host import may call back into the VM, and the
/// nested run shares the call depth and the fuel of the outermost one.
pub struct Vm {
    program: Rc<Program>,
    types: TypeTable,
    pub(crate) depth: u32,
    pub(crate) fuel: u64,
    fuel_per_run: u64,
    nesting: u32,
    /// A debugger's instruction hook, checked before every instruction while installed.
    pub(crate) hook: Option<Box<dyn InstrHook>>,
}

impl fmt::Debug for Vm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Vm")
            .field("program", &self.program)
            .field("types", &self.types)
            .field("depth", &self.depth)
            .field("fuel", &self.fuel)
            .field("fuel_per_run", &self.fuel_per_run)
            .field("nesting", &self.nesting)
            .field("hook", &self.hook.is_some())
            .finish()
    }
}

impl Vm {
    pub fn new(program: Rc<Program>) -> Vm {
        Vm {
            types: TypeTable::new(program.clone()),
            program,
            depth: 0,
            fuel: FUEL_PER_RUN,
            fuel_per_run: FUEL_PER_RUN,
            nesting: 0,
            hook: None,
        }
    }

    pub fn program(&self) -> &Rc<Program> {
        &self.program
    }

    /// Starts a run: the outermost one refills the fuel.
    pub(crate) fn enter(&mut self) {
        if self.nesting == 0 {
            self.fuel = self.fuel_per_run;
        }
        self.nesting = self.nesting.saturating_add(1);
    }

    pub(crate) fn leave(&mut self) {
        self.nesting = self.nesting.saturating_sub(1);
    }

    /// Runs `f` with the fuel accounting of a top-level entry: each run started inside `f` at
    /// its outermost level gets a full budget, as if no run were in progress. The enclosing
    /// run's remaining fuel is restored afterwards, so the host's own work (a save replay
    /// inside an import) neither spends nor refills the budget of the run that called it.
    pub fn with_fresh_budget<R>(&mut self, f: impl FnOnce(&mut Vm) -> R) -> R {
        let fuel = self.fuel;
        let nesting = std::mem::replace(&mut self.nesting, 0);
        let result = f(self);
        self.fuel = fuel;
        self.nesting = nesting;
        result
    }

    /// Runs a non-scene function to completion. Re-entrant: host imports may call it.
    pub fn call(
        &mut self,
        host: &mut dyn Host,
        func: FuncId,
        args: Vec<Value>,
    ) -> Result<Value, Trap> {
        let fits = self
            .program
            .function(func)
            .is_some_and(|f| f.kind != FnKind::Scene && usize::from(f.params) == args.len());
        if !fits {
            return Err(Trap::new(TrapKind::KindMismatch));
        }
        self.enter();
        let result = self.run_frame(host, func, args);
        self.leave();
        result
    }

    /// CALLVAL semantics on a Func, Closure or HostFn value.
    pub fn call_value(
        &mut self,
        host: &mut dyn Host,
        callee: &Value,
        kind: FnKind,
        args: Vec<Value>,
    ) -> Result<Value, Trap> {
        let callee = match callee {
            Value::Ref(r) => refs::read(r)?,
            Value::Uninit => return Err(Trap::new(TrapKind::ReadUninit)),
            v => v.clone(),
        };
        let callee = exec::callee(&self.program, callee, kind, args)?;
        self.enter();
        let result = match callee {
            Callee::Frame(func, args) => self.run_frame(host, func, args),
            Callee::Host(f, args) => host.call_host_fn(self, &f, args),
        };
        self.leave();
        result
    }

    /// Pushes a frame of `func` on a fresh stack and runs it to its return.
    fn run_frame(
        &mut self,
        host: &mut dyn Host,
        func: FuncId,
        args: Vec<Value>,
    ) -> Result<Value, Trap> {
        let mut stack = Vec::new();
        exec::push_frame(self, &mut stack, func, args)?;
        match exec::run(self, host, &mut stack, None)? {
            Exit::Return(value) => Ok(value),
            Exit::Step(..) => Err(Trap::new(TrapKind::KindMismatch)),
        }
    }

    /// Reads through a reference (auto-deref). Errors on host-part roots.
    pub fn load(&self, r: &Ref) -> Result<Value, Trap> {
        refs::read(r)
    }

    /// Runs `f` on the value a cell-rooted reference points to.
    pub fn with_mut<R>(&self, r: &Ref, f: impl FnOnce(&mut Value) -> R) -> Result<R, Trap> {
        refs::with_mut(r, f)
    }

    pub fn types(&self) -> &TypeTable {
        &self.types
    }

    /// `TypeTable::resolve` needs `&mut`; boot resolves the std types through this.
    pub fn types_mut(&mut self) -> &mut TypeTable {
        &mut self.types
    }

    /// The instruction budget of each top-level run (default [`FUEL_PER_RUN`]).
    pub fn set_fuel_per_run(&mut self, fuel: u64) {
        self.fuel_per_run = fuel;
    }

    /// Installs (or removes) the instruction hook; returns the previous one. While installed,
    /// every run (story, call, nested) asks it before each instruction.
    pub fn set_hook(&mut self, hook: Option<Box<dyn InstrHook>>) -> Option<Box<dyn InstrHook>> {
        std::mem::replace(&mut self.hook, hook)
    }
}
