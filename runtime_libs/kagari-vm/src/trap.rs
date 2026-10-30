//! VM panics: a defined stop with its kind and the frames it unwound.

use std::fmt;

use kagari_bytecode::FuncId;

use crate::program::Program;

/// A VM panic. `backtrace` lists `(function, instruction index)` innermost first; `step` is
/// the last step the story fiber reached before a trap in story code.
#[derive(Clone, Debug)]
pub struct Trap {
    pub kind: TrapKind,
    pub backtrace: Vec<(FuncId, u32)>,
    pub step: Option<StepId>,
}

/// A step ID: the function of a STEP and its id, the step's ordinal among the step forms of
/// the function's source item (`Program::step_path` prints `{path}#{id}`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StepId {
    pub func: FuncId,
    pub id: u32,
}

/// Why the VM panicked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrapKind {
    Overflow,
    DivisionByZero,
    IndexOutOfRange,
    ReadUninit,
    TagMismatch {
        expected: &'static str,
        found: &'static str,
    },
    KindMismatch,
    CallDepth,
    Unreachable,
    HostPartAccess,
    FuelExhausted,
    ClosureEquality,
    /// Raised by an import.
    Host(String),
}

impl fmt::Display for TrapKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrapKind::Overflow => f.write_str("integer overflow or result out of range"),
            TrapKind::DivisionByZero => f.write_str("division by zero"),
            TrapKind::IndexOutOfRange => f.write_str("index out of range"),
            TrapKind::ReadUninit => f.write_str("read of an uninitialized value"),
            TrapKind::TagMismatch { expected, found } => {
                write!(f, "type mismatch: expected {expected}, found {found}")
            }
            TrapKind::KindMismatch => f.write_str("function kind mismatch"),
            TrapKind::CallDepth => f.write_str("call depth limit exceeded"),
            TrapKind::Unreachable => f.write_str("unreachable code reached"),
            TrapKind::HostPartAccess => f.write_str("host part accessed through a reference"),
            TrapKind::FuelExhausted => f.write_str("instruction budget exhausted"),
            TrapKind::ClosureEquality => f.write_str("closures cannot be compared"),
            TrapKind::Host(message) => f.write_str(message),
        }
    }
}

impl Trap {
    /// A trap with an empty backtrace.
    pub fn new(kind: TrapKind) -> Trap {
        Trap {
            kind,
            backtrace: Vec::new(),
            step: None,
        }
    }

    /// A trap raised by an import.
    pub fn host(message: impl Into<String>) -> Trap {
        Trap::new(TrapKind::Host(message.into()))
    }

    /// `vm panic: {kind} at {path} ({loc})`, then `, after step {step id}` when the story had
    /// reached a step, then `\n  called from {path} ({loc})` per outer frame; `loc` is the
    /// source location, else `instruction {i}`.
    pub fn render(&self, program: &Program) -> String {
        let frame = |func: FuncId, instr: u32| {
            let loc = program
                .location(func, instr)
                .unwrap_or_else(|| format!("instruction {instr}"));
            format!("{} ({loc})", program.function_path(func))
        };
        let mut out = format!("vm panic: {}", self.kind);
        let mut frames = self.backtrace.iter();
        if let Some(&(func, instr)) = frames.next() {
            out.push_str(&format!(" at {}", frame(func, instr)));
        }
        if let Some(step) = self.step {
            out.push_str(&format!(", after step {}", program.step_path(step)));
        }
        for &(func, instr) in frames {
            out.push_str(&format!("\n  called from {}", frame(func, instr)));
        }
        out
    }
}

impl fmt::Display for Trap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "vm panic: {}", self.kind)?;
        for (func, instr) in &self.backtrace {
            write!(f, "\n  at function {}, instruction {instr}", func.0)?;
        }
        if let Some(step) = self.step {
            write!(f, "\n  after step {} of function {}", step.id, step.func.0)?;
        }
        Ok(())
    }
}

impl std::error::Error for Trap {}

/// An internal fault: something verification should have ruled out.
pub(crate) fn fault() -> Trap {
    Trap::new(TrapKind::Unreachable)
}

/// A value of the wrong tag.
pub(crate) fn mismatch(expected: &'static str, found: &'static str) -> Trap {
    Trap::new(TrapKind::TagMismatch { expected, found })
}
