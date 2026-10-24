//! The story as a recursive state machine over finite data.
//!
//! 1. Every scene body is lowered to a control-flow graph whose suspension points are the
//!    steps and the scene calls ([`cfg`]).
//! 2. [`termination`] checks that every loop and every cycle of calls passes a step.
//! 3. [`explore`] computes the reachable states from every entry scene and every argument
//!    value, summarizing each scene per entry state (call/return, as in the reachability
//!    of recursive state machines). A state is a position, the live slots of the scene's
//!    frame and the values of the story variables. Instant code between two suspensions
//!    runs in the const-evaluation interpreter ([`exec`]). This finds stuck choices and
//!    errors in story code, and leaves unreached steps and scenes.
//! 4. [`screens`] runs the renders over the reachable states: the screens that reach each
//!    step (bounded by [`SCREEN_CAP`]), and whether the screen a step shows is determined by
//!    the story state (consistency).

mod cfg;
mod exec;
mod explore;
mod screens;
mod termination;

use rustc_hash::FxHashMap;

use crate::db::Db;
use crate::def_map::{CrateId, def_maps};
use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::eval::interp::{BodyRef, Ctx};
use crate::eval::std_items::StdTable;
use crate::hir::ExprId;
use crate::input::Program;
use crate::item_tree::{FnKind, ItemId, ItemKind, item_tree};
use crate::signatures::program_signatures;

use cfg::StoryBody;

/// The most states the exploration may visit (*tentative*: a compiler setting). More is
/// `analysis/too-many-states`.
pub const STATE_CAP: usize = 200_000;

/// The most distinct screens that may reach one step (*tentative*: the cap is an open
/// question). A render that makes a step's set grow past it is `analysis/unbounded-screens`.
pub const SCREEN_CAP: usize = 64;

/// A lowered scene.
struct SceneData<'db> {
    pub item: ItemId,
    pub b: BodyRef<'db>,
    pub body: StoryBody,
    pub game: bool,
}

impl SceneData<'_> {
    pub fn name(&self, db: &dyn Db) -> String {
        item_tree(db, self.item.file)
            .item(self.item.index)
            .and_then(|item| item.name.as_ref())
            .map(ToString::to_string)
            .unwrap_or_default()
    }

    pub fn diagnostic(
        &self,
        db: &dyn Db,
        expr: ExprId,
        code: DiagnosticCode,
        message: impl Into<String>,
    ) -> Diagnostic {
        Diagnostic::new(
            self.item.file.path(db).clone(),
            self.b.body.store.expr_range(expr),
            code,
            message,
        )
    }
}

/// Every scene of the program, in load order.
struct Scenes<'db> {
    pub list: Vec<SceneData<'db>>,
    pub index: FxHashMap<ItemId, usize>,
}

pub(super) fn check(db: &dyn Db, program: Program) -> Vec<Diagnostic> {
    let Some(std) = StdTable::new(db, program) else {
        return Vec::new();
    };
    let sigs = program_signatures(db, program);
    let ctx = Ctx::new(db, program, &std, sigs);
    let maps = def_maps(db, program);
    let mut scenes = Scenes {
        list: Vec::new(),
        index: FxHashMap::default(),
    };
    let mut vars = Vec::new();
    for &(file, krate) in &maps.files {
        let tree = item_tree(db, file);
        for (index, data) in tree.items.iter().enumerate() {
            let Ok(index) = u32::try_from(index) else {
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
                    let Some(b) = ctx.body(item) else {
                        continue;
                    };
                    let body = cfg::lower(b, sigs, std.push_step);
                    scenes.index.insert(item, scenes.list.len());
                    scenes.list.push(SceneData {
                        item,
                        b,
                        body,
                        game: krate == CrateId::Game,
                    });
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    if !termination::check(db, &scenes, sigs, &mut out) {
        // The other analyses run the story, which must reach its steps.
        return out;
    }
    let explored = explore::explore(db, &ctx, &scenes, &vars, &mut out);
    if let Some(explored) = explored {
        screens::check(db, &ctx, &scenes, &explored, &mut out);
        explored.report_reachability(db, &scenes, &mut out);
    }
    out
}
