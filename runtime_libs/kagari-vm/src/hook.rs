//! Debugger introspection: views of frames and the per-instruction hook.

use kagari_bytecode::FuncId;

use crate::program::Program;
use crate::story::StoryStatics;
use crate::types::TypeTable;
use crate::value::Value;

/// A frame of a running or suspended stack, for debuggers.
#[derive(Clone, Copy, Debug)]
pub struct FrameView<'a> {
    pub func: FuncId,
    /// The index of the frame's current instruction: a caller's stays on its CALL, a suspended
    /// fiber's top frame on its STEP.
    pub pc: u32,
    pub regs: &'a [Value],
}

/// A debugger's hook into the instruction loop (installed with [`Vm::set_hook`]).
///
/// [`Vm::set_hook`]: crate::Vm::set_hook
pub trait InstrHook {
    /// Called before every instruction while installed: whether to stop before instruction
    /// `pc` of `func`. `depth` is the VM call depth (shared by nested runs). Keep it cheap.
    fn check(&mut self, func: FuncId, pc: u32, depth: u32) -> bool;
    /// Stopped before `stop.frames.last()`'s instruction; may block (a debugger serving
    /// requests). The hook is detached from the VM during the call, so it is never re-entered.
    fn stop(&mut self, stop: &HookStop<'_>);
}

/// What a hook sees when it stops.
#[derive(Clone, Copy, Debug)]
pub struct HookStop<'a> {
    pub program: &'a Program,
    pub types: &'a TypeTable,
    /// The frames of the current run, bottom first (for a story run: the fiber's frames).
    pub frames: &'a [FrameView<'a>],
    /// The story variables when the current run is the story fiber's.
    pub statics: Option<&'a StoryStatics>,
    pub depth: u32,
}
