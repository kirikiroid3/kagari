//! Instruction breakpoints and stepping inside code: the VM's [`InstrHook`].
//!
//! The hook stops before an instruction at an instruction breakpoint, or where a code step
//! (`next`, `stepIn`, `stepOut` from a stop inside code) ends: at the next source position in
//! the same or an outer frame, at any next position, or in an outer frame. A code step that
//! starts in story code stays in story runs (the fiber and what it calls), and when the story
//! reaches a step it ends there; breakpoints stop anywhere but in system code (`fn`). A stop
//! blocks inside the VM and serves the protocol until the user goes on
//! ([`crate::server::serve_code_stop`]).

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

use kagari_bytecode::FnKind;
use kagari_vm::{FuncId, HookStop, InstrHook, Program};

use crate::frames::Artifact;
use crate::protocol::{BreakpointsBody, InstructionPoint};
use crate::transport::Connection;

/// Where a code step ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodeStep {
    /// At the next source position at the same or a smaller depth.
    Over {
        depth: u32,
        func: FuncId,
        offset: Option<u32>,
    },
    /// At the next source position at any depth.
    In {
        depth: u32,
        func: FuncId,
        offset: Option<u32>,
    },
    /// At a smaller depth.
    Out { depth: u32 },
}

/// What the hook stops at; shared with the server.
pub struct HookState {
    /// Off while the debugger drives the story itself (positioning, lookahead).
    pub armed: bool,
    pub breakpoints: HashSet<(FuncId, u32)>,
    pub stepping: Option<CodeStep>,
    /// The code step started in a story run (the fiber), not in a render the engine called.
    step_in_story: bool,
    /// The engine's pause flag (`Engine::debug_pause_flag`): a code step that leaves the story
    /// run at a step ends there.
    pub pause_flag: Rc<Cell<bool>>,
    /// Per function: whether a code step may stop in it (story code with a source file outside
    /// the std directory).
    steppable: Vec<bool>,
    program: Rc<Program>,
    /// Step breakpoints requested while stopped inside code, for the server to apply.
    pub pending_steps: Option<Vec<String>>,
    /// The client disconnected while stopped inside code.
    pub detached: bool,
}

impl HookState {
    pub fn new(program: Rc<Program>, pause_flag: Rc<Cell<bool>>) -> HookState {
        let steppable = (0..program.module().functions.len())
            .map(|index| {
                let Ok(index) = u32::try_from(index) else {
                    return false;
                };
                let f = FuncId(index);
                program.kind(f).is_some_and(|k| k != FnKind::Fn)
                    && program
                        .file(f)
                        .is_some_and(|file| !file.starts_with("std/"))
            })
            .collect();
        HookState {
            armed: true,
            breakpoints: HashSet::new(),
            stepping: None,
            step_in_story: false,
            pause_flag,
            steppable,
            program,
            pending_steps: None,
            detached: false,
        }
    }

    pub fn program(&self) -> &Rc<Program> {
        &self.program
    }

    /// Replaces the instruction breakpoints; one verdict per point. System code (`fn`) has
    /// none.
    pub fn set_points(&mut self, points: &[InstructionPoint]) -> BreakpointsBody {
        let program = self.program.clone();
        let resolved: Vec<Option<(FuncId, u32)>> = points
            .iter()
            .map(|p| {
                let f = program.find_function(&p.function)?;
                (program.kind(f)? != FnKind::Fn).then_some(())?;
                Some((f, program.instr_at_word(f, p.word)?))
            })
            .collect();
        self.breakpoints = resolved.iter().flatten().copied().collect();
        BreakpointsBody {
            verified: resolved.iter().map(Option::is_some).collect(),
        }
    }

    fn steppable(&self, func: FuncId) -> bool {
        self.steppable
            .get(func.0 as usize)
            .copied()
            .unwrap_or(false)
    }

    /// Whether to stop before instruction `pc` of `func` at `depth`.
    fn wants(&self, func: FuncId, pc: u32, depth: u32) -> bool {
        if !self.armed {
            return false;
        }
        if self.breakpoints.contains(&(func, pc)) {
            return true;
        }
        let Some(step) = self.stepping else {
            return false;
        };
        if !self.steppable(func) {
            return false;
        }
        let moved = |from: FuncId, offset: Option<u32>| {
            func != from || self.program.offset(func, pc) != offset
        };
        match step {
            CodeStep::Out { depth: d } => depth < d,
            CodeStep::Over {
                depth: d,
                func: from,
                offset,
            } => depth < d || (depth == d && moved(from, offset)),
            CodeStep::In {
                depth: d,
                func: from,
                offset,
            } => depth != d || moved(from, offset),
        }
    }

    /// A code step (`next`, `stepIn` or `stepOut`) from the top of `stop`.
    pub fn step_from(&mut self, stop: &HookStop<'_>, command: &str) {
        let Some(top) = stop.frames.last() else {
            return;
        };
        let offset = stop.program.offset(top.func, top.pc);
        self.stepping = Some(match command {
            crate::protocol::command::STEP_IN => CodeStep::In {
                depth: stop.depth,
                func: top.func,
                offset,
            },
            crate::protocol::command::STEP_OUT => CodeStep::Out { depth: stop.depth },
            _ => CodeStep::Over {
                depth: stop.depth,
                func: top.func,
                offset,
            },
        });
        // Leaving the story run at a step ends the code step there.
        self.step_in_story = stop.statics.is_some();
        self.pause_flag.set(self.step_in_story);
    }

    /// Whether a code step from story code is under way.
    pub fn story_step(&self) -> bool {
        self.stepping.is_some() && self.step_in_story
    }

    /// Ends any code step.
    pub fn clear_step(&mut self) {
        self.stepping = None;
        self.step_in_story = false;
        self.pause_flag.set(false);
    }
}

/// The hook installed in the VM.
pub struct Hook {
    pub state: Rc<RefCell<HookState>>,
    pub conn: Rc<Connection>,
    pub artifact: Option<Rc<Artifact>>,
}

impl InstrHook for Hook {
    fn check(&mut self, func: FuncId, pc: u32, depth: u32) -> bool {
        self.state
            .try_borrow()
            .is_ok_and(|state| state.wants(func, pc, depth))
    }

    fn stop(&mut self, stop: &HookStop<'_>) {
        let (breakpoint, story_step) = {
            let Ok(state) = self.state.try_borrow() else {
                return;
            };
            let breakpoint = stop
                .frames
                .last()
                .is_some_and(|top| state.breakpoints.contains(&(top.func, top.pc)));
            (breakpoint, state.story_step())
        };
        // A code step from story code stays in story runs; a render the engine calls is not
        // one.
        if !breakpoint && story_step && stop.statics.is_none() {
            return;
        }
        if let Ok(mut state) = self.state.try_borrow_mut() {
            state.clear_step();
        }
        crate::server::serve_code_stop(self, stop, breakpoint);
    }
}
