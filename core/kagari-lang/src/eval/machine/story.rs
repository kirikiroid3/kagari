//! Story mode of the MIR executor: running a scene body up to its next suspension
//! (`Step`, `Rng`, `SceneCall`) or its `Return`, for the story analysis. The scene's frame
//! stays with the caller, which reads the suspension's operands and the kept locals from it.
//!
//! The story variables live outside the machine ([`StoryValues`]); a variable gets a heap
//! cell, thawed from its value, the first time a place reaches it, so a run touches only the
//! variables it uses and the caller re-freezes only those ([`StoryVars::accessed`]).

use rustc_hash::FxHashMap;

use crate::diagnostic::{Anchor, NodeId, Owner};
use crate::eval::ConstValue;
use crate::hir::ExprId;
use crate::item_tree::ItemId;
use crate::mir::{self, Operand, StatementKind};

use super::exec::{Frame, Next};
use super::value::Value;
use super::{EvalError, Machine, Place};

/// The values of the story variables a run starts from, by position.
pub(crate) trait StoryValues {
    fn value(&self, slot: usize) -> Option<&ConstValue>;
}

/// The story variables of a story-mode run.
pub(crate) struct StoryVars<'a> {
    /// The position of each story variable.
    slots: &'a FxHashMap<ItemId, usize>,
    values: &'a dyn StoryValues,
    /// The heap cell of each story variable accessed so far, by position.
    cells: Vec<Option<usize>>,
}

impl<'a> StoryVars<'a> {
    pub fn new(slots: &'a FxHashMap<ItemId, usize>, values: &'a dyn StoryValues) -> Self {
        StoryVars {
            slots,
            values,
            cells: vec![None; slots.len()],
        }
    }

    /// The position and heap cell of every story variable accessed so far.
    pub fn accessed(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.cells
            .iter()
            .enumerate()
            .filter_map(|(slot, cell)| Some((slot, (*cell)?)))
    }
}

impl<'db> Machine<'db> {
    /// The heap cell of story variable `item`, allocated on first access; `None` outside
    /// story mode.
    pub(super) fn story_cell(&mut self, item: ItemId) -> Option<usize> {
        let story = self.story.as_ref()?;
        let slot = *story.slots.get(&item)?;
        if let Some(cell) = *story.cells.get(slot)? {
            return Some(cell);
        }
        let value = self.thaw(story.values.value(slot)?);
        let cell = self.alloc(value);
        *self.story.as_mut()?.cells.get_mut(slot)? = Some(cell);
        Some(cell)
    }

    /// Runs `scene` until it reaches a terminator the executor does not run itself
    /// (a suspension, `Return` or `Unreachable`), entering a block at most `fuel` times.
    /// `last` is the source of the last statement or terminator of `scene` started.
    /// `Ok(false)` when the fuel ran out.
    pub(crate) fn run_scene(
        &mut self,
        scene: &mut Frame<'db>,
        fuel: &mut u32,
        last: &mut Option<ExprId>,
    ) -> Result<bool, EvalError> {
        loop {
            let body = scene.body;
            let block = &body.blocks[scene.block];
            let at_term = scene.stmt >= block.stmts.len();
            if at_term {
                if block.term_src.is_some() {
                    *last = block.term_src;
                }
                if block.term.is_suspension()
                    || matches!(
                        block.term,
                        mir::Terminator::Return | mir::Terminator::Unreachable
                    )
                {
                    return Ok(true);
                }
            } else if let Some(src) = block.stmts.get(scene.stmt).and_then(|stmt| stmt.src) {
                *last = Some(src);
            }
            match self.step(scene)? {
                Next::Continue => {
                    if at_term {
                        match fuel.checked_sub(1) {
                            Some(left) => *fuel = left,
                            None => return Ok(false),
                        }
                    }
                }
                Next::Enter(callee) => {
                    let block = &body.blocks[scene.block];
                    let Some(stmt) = block.stmts.get(scene.stmt) else {
                        return Err(EvalError::abort());
                    };
                    let StatementKind::Call { dest, .. } = &stmt.kind else {
                        return Err(EvalError::abort());
                    };
                    let at = self.scene_anchor(scene, stmt.src);
                    self.push_depth(at)?;
                    let value = self.execute(callee);
                    self.depth = self.depth.saturating_sub(1);
                    let value = value?;
                    let place = self.place(scene, dest, at)?;
                    self.write_place(&place, value)
                        .map_err(|kind| EvalError { kind, at })?;
                    scene.stmt = scene.stmt.saturating_add(1);
                }
                Next::Return(_) => return Err(EvalError::abort()),
            }
        }
    }

    /// The value of an operand of `scene`.
    pub(crate) fn scene_operand(
        &mut self,
        scene: &Frame<'db>,
        operand: &Operand,
        src: Option<ExprId>,
    ) -> Result<Value, EvalError> {
        let at = self.scene_anchor(scene, src);
        self.operand(scene, operand, at)
    }

    /// The machine place of a place of `scene`.
    pub(crate) fn scene_place(
        &mut self,
        scene: &Frame<'db>,
        place: &mir::Place,
        src: Option<ExprId>,
    ) -> Result<Place, EvalError> {
        let at = self.scene_anchor(scene, src);
        self.place(scene, place, at)
    }

    fn scene_anchor(&self, scene: &Frame<'db>, src: Option<ExprId>) -> Option<Anchor> {
        src.map(|expr| Anchor::Node(Owner::Body(scene.body.source.owner()), NodeId::Expr(expr)))
    }
}
