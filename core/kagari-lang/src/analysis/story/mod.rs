//! The story as a recursive state machine over finite data.
//!
//! 1. Every scene body is a MIR body whose suspension terminators are the steps, the
//!    random draws and the scene calls; [`liveness`] computes what each suspension keeps.
//! 2. [`termination`] checks that every loop and every cycle of calls passes a step.
//! 3. [`explore`] computes the reachable states from every entry scene and every argument
//!    value, summarizing each scene per entry state (call/return, as in the reachability
//!    of recursive state machines). A state is a position, the kept locals of the scene's
//!    frame and the values of the story variables. Instant code between two suspensions
//!    runs in the MIR executor ([`segment`]). This finds stuck choices and errors in story
//!    code, and leaves unreached steps and scenes.
//! 4. [`screens`] runs the renders over the reachable states: the screens that reach each
//!    step (bounded by [`SCREEN_CAP`]), and whether the screen a step shows is determined by
//!    the story state (consistency).

mod explore;
mod keyframes;
mod liveness;
mod screens;
mod segment;
mod termination;
#[cfg(test)]
mod tests;

use rustc_hash::FxHashMap;

use crate::db::Db;
use crate::def_map::{CrateId, def_maps};
use crate::diagnostic::{Anchor, Diag, DiagnosticCode, NodeId, Owner};
use crate::eval::std_items::std_table;
use crate::hir::ExprId;
use crate::input::Program;
use crate::item_tree::ItemIndex;
use crate::item_tree::{FnKind, ItemId, ItemKind, item_tree};
use crate::mir::{BasicBlock, ItemMir, LocalSet, StepId, Terminator, item_mir};
use crate::paths;
use crate::signatures::program_signatures;

use explore::StoryCx;
use liveness::scene_kept;
use segment::SceneBody;

/// The most distinct screens that may reach one step (*tentative*: the cap is an open
/// question). A render that makes a step's set grow past it is `E5005`.
pub const SCREEN_CAP: usize = 64;

/// A scene with its MIR.
struct SceneData<'db> {
    pub item: ItemId,
    pub mir: &'db ItemMir,
    /// The kept locals of every suspension block of `mir.main`.
    pub kept: &'db FxHashMap<BasicBlock, LocalSet>,
    pub game: bool,
}

impl<'db> SceneData<'db> {
    pub fn body(&self) -> SceneBody<'db> {
        SceneBody {
            body: &self.mir.main,
            kept: self.kept,
        }
    }

    pub fn name(&self, db: &dyn Db) -> String {
        item_tree(db, self.item.file)
            .item(self.item.index)
            .and_then(|item| item.name.as_ref())
            .map(ToString::to_string)
            .unwrap_or_default()
    }

    /// The anchor of an expression of the scene's body.
    pub fn anchor(&self, expr: ExprId) -> Anchor {
        Anchor::Node(Owner::Body(self.item), NodeId::Expr(expr))
    }

    pub fn diagnostic(
        &self,
        expr: ExprId,
        code: DiagnosticCode,
        message: impl Into<String>,
    ) -> Diag {
        Diag::new(self.anchor(expr), code, message)
    }

    /// The step ID of the step whose `push_step` call is `expr`.
    pub fn step_id(&self, expr: ExprId) -> Option<StepId> {
        self.mir
            .main
            .blocks
            .iter()
            .find_map(|(_, data)| match data.term {
                Terminator::Step { id, .. } if data.term_src == Some(expr) => Some(id),
                _ => None,
            })
    }

    /// A diagnostic at the step `expr`, with its step ID (`{path}#{ordinal}`, the name the
    /// bytecode and the runtime use) as related information.
    pub fn step_diagnostic(
        &self,
        db: &dyn Db,
        program: Program,
        expr: ExprId,
        code: DiagnosticCode,
        message: impl Into<String>,
    ) -> Diag {
        let diagnostic = self.diagnostic(expr, code, message);
        match self.step_id(expr) {
            Some(id) => {
                let path = paths::assoc_path(db, def_maps(db, program), self.item);
                diagnostic.with_related(self.anchor(expr), format!("step `{path}{id}`"))
            }
            None => diagnostic,
        }
    }
}

/// Every scene of the program, in load order.
struct Scenes<'db> {
    pub list: Vec<SceneData<'db>>,
    pub index: FxHashMap<ItemId, usize>,
}

pub(super) fn check(db: &dyn Db, program: Program) -> Vec<Diag> {
    let Some(std) = std_table(db, program).as_ref() else {
        return Vec::new();
    };
    let sigs = program_signatures(db, program);
    let cx = StoryCx {
        db,
        program,
        std,
        sigs,
    };
    let maps = def_maps(db, program);
    let mut scenes = Scenes {
        list: Vec::new(),
        index: FxHashMap::default(),
    };
    let mut vars = Vec::new();
    for &(file, krate) in &maps.files {
        let tree = item_tree(db, file);
        for (index, data) in tree.items.iter().enumerate() {
            let Some(index) = ItemIndex::from_usize(index) else {
                continue;
            };
            let item = ItemId { file, index };
            match data.kind {
                ItemKind::Static { mutable: true } => vars.push(item),
                ItemKind::Fn {
                    kind: FnKind::Pushdown,
                    has_body: true,
                    ..
                } if sigs.fn_sig(item).is_some_and(|sig| sig.scene) => {
                    let (Some(mir), Some(kept)) = (
                        item_mir(db, program, item).as_ref(),
                        scene_kept(db, program, item).as_ref(),
                    ) else {
                        continue;
                    };
                    scenes.index.insert(item, scenes.list.len());
                    scenes.list.push(SceneData {
                        item,
                        mir,
                        kept,
                        game: krate == CrateId::Game,
                    });
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    if !termination::check(db, program, &scenes, sigs, &mut out) {
        // The other analyses run the story, which must reach its steps.
        return out;
    }
    let explored = explore::explore(cx, &scenes, &vars, &mut out);
    if let Some(explored) = explored {
        screens::check(cx, &scenes, &explored, &mut out);
        explored.report_reachability(db, program, &scenes, &mut out);
        if program.options(db).keyframe_check {
            out.extend(keyframes::check(db, program, &explored, &scenes));
        }
    }
    out
}
