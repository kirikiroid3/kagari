//! The diagnostics of the exploration: faults, given-up paths, stuck choices and
//! unreachable steps and scenes.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::def_map::def_maps;
use crate::diagnostic::{Anchor, Diag, DiagnosticCode};
use crate::eval::machine::{ErrorKind, EvalError};
use crate::eval::{AssetKind, ConstValue, TABLE_DOMAIN_CAP, const_eval, display_value};
use crate::hir::{Expr, ExprId, body_of};
use crate::infer::{ValueRef, inference};
use crate::item_tree::{ItemId, ItemIndex, item_tree};
use crate::mir::{BasicBlock, Terminator};
use crate::signatures::{FnSig, ItemSig};

use super::super::Scenes;
use super::{Explored, Explorer, GlobalsId, StoryCx};

impl Explorer<'_, '_> {
    /// The exploration cannot continue a path: some state may be missed, so nothing is
    /// claimed unreachable, and the place is reported.
    pub(super) fn give_up(&mut self, scene: usize, at: Option<ExprId>) {
        self.out.complete = false;
        let Some(data) = self.scenes.list.get(scene) else {
            return;
        };
        let message = "the story analysis cannot follow the story past this point (it holds a \
                       value the analysis cannot store, such as a closure that captures \
                       locals, across a step), so unreachable steps and scenes are not reported";
        let diagnostic = match at {
            Some(expr) => data.diagnostic(expr, DiagnosticCode::AnalysisIncomplete, message),
            None => name_diagnostic(data.item, DiagnosticCode::AnalysisIncomplete, message),
        };
        if !self.faults.contains(&diagnostic) {
            self.faults.push(diagnostic);
        }
    }

    /// A draw whose type has too many values.
    pub(super) fn too_many_draws(&mut self, scene: usize, at: Option<ExprId>) {
        self.out.complete = false;
        let (Some(data), Some(expr)) = (self.scenes.list.get(scene), at) else {
            return;
        };
        let diagnostic = data.diagnostic(
            expr,
            DiagnosticCode::AnalysisTooManyStates,
            format!(
                "this draw has too many values to analyze: more than the limit of \
                 {TABLE_DOMAIN_CAP}, so unreachable steps and scenes are not reported"
            ),
        );
        if !self.faults.contains(&diagnostic) {
            self.faults.push(diagnostic);
        }
    }

    pub(super) fn fault(&mut self, scene: usize, error: &EvalError, at: Option<ExprId>) {
        let ErrorKind::Fault(fault) = &error.kind else {
            return;
        };
        let Some(data) = self.scenes.list.get(scene) else {
            return;
        };
        let diagnostic = match (at, error.at) {
            (Some(expr), failed) => {
                let anchor = data.anchor(expr);
                let mut diagnostic = Diag::new(anchor, fault.code, fault.message.clone());
                if let Some(failed) = failed
                    && failed != anchor
                {
                    diagnostic = diagnostic.with_related(failed, "evaluation failed here");
                }
                diagnostic
            }
            (None, Some(failed)) => Diag::new(failed, fault.code, fault.message.clone()),
            (None, None) => return,
        };
        if !self.faults.contains(&diagnostic) {
            self.faults.push(diagnostic);
        }
    }

    pub(super) fn report_stuck(&self, out: &mut Vec<Diag>) {
        let mut stuck: Vec<_> = self.stuck.iter().collect();
        stuck.sort_by_key(|((scene, expr), _)| (*scene, expr.into_raw().into_u32()));
        for (&(scene, expr), &globals) in stuck {
            let Some(data) = self.scenes.list.get(scene) else {
                continue;
            };
            if !data.game {
                continue;
            }
            let mut message = "this choice can be reached with every arm hidden: the player would \
                               have nothing to select"
                .to_owned();
            let witness = self.witness(scene, expr, globals);
            if !witness.is_empty() {
                message.push_str(&format!(" (for example when {})", witness.join(", ")));
            }
            out.push(data.step_diagnostic(
                self.cx.db,
                self.cx.program,
                expr,
                DiagnosticCode::AnalysisStuckChoice,
                message,
            ));
        }
    }

    /// The story variables the choice `expr` reads, with their values in a stuck state.
    fn witness(&self, scene: usize, expr: ExprId, globals: GlobalsId) -> Vec<String> {
        let db = self.cx.db;
        let (Some(data), Some(values)) =
            (self.scenes.list.get(scene), self.out.globals.get(globals.0))
        else {
            return Vec::new();
        };
        let (Some(body), Some(infer)) = (
            body_of(db, data.item),
            inference(db, self.cx.program, data.item),
        ) else {
            return Vec::new();
        };
        let store = &body.store;
        let mut read = Vec::new();
        let mut stack = vec![expr];
        while let Some(expr) = stack.pop() {
            if let Some(ValueRef::Static(item)) = infer.paths.get(&expr)
                && !read.contains(item)
            {
                read.push(*item);
            }
            if !matches!(store.expr(expr), Expr::Closure { .. }) {
                stack.extend(crate::eval::const_exprs::children(store.expr(expr)));
            }
        }
        read.sort_by_key(|item| (item.file.path(db).clone(), item.index));
        read.iter()
            .filter_map(|item| {
                let index = self.out.vars.iter().position(|var| var == item)?;
                let name = item_tree(db, item.file).item(item.index)?.name.clone()?;
                let value = self.out.values.get(*values.get(index)?)?;
                Some(format!("`{name}` is `{}`", display_value(db, value)))
            })
            .collect()
    }
}

impl Explored {
    /// Unreachable scenes and steps, when the exploration was complete.
    pub fn report_reachability(
        &self,
        db: &dyn crate::db::Db,
        program: crate::input::Program,
        scenes: &Scenes<'_>,
        out: &mut Vec<Diag>,
    ) {
        if !self.complete {
            return;
        }
        let mut reached_blocks: FxHashMap<usize, FxHashSet<BasicBlock>> = FxHashMap::default();
        for context in &self.contexts {
            let blocks = reached_blocks.entry(context.scene).or_default();
            blocks.extend(context.nodes.iter().map(|node| node.block));
        }
        for (index, scene) in scenes.list.iter().enumerate() {
            if !scene.game {
                continue;
            }
            let Some(blocks) = reached_blocks.get(&index) else {
                out.push(name_diagnostic(
                    scene.item,
                    DiagnosticCode::AnalysisUnreachableScene,
                    format!(
                        "the scene `{}` is never reached from an entry scene",
                        scene.name(db)
                    ),
                ));
                continue;
            };
            for (block, data) in scene.mir.main.blocks.iter() {
                if let (Terminator::Step { .. }, Some(expr)) = (&data.term, data.term_src)
                    && !blocks.contains(&block)
                {
                    out.push(scene.step_diagnostic(
                        db,
                        program,
                        expr,
                        DiagnosticCode::AnalysisUnreachableStep,
                        "this step is never reached",
                    ));
                }
            }
        }
    }
}

pub(super) fn name_diagnostic(
    item: ItemId,
    code: DiagnosticCode,
    message: impl Into<String>,
) -> Diag {
    Diag::new(Anchor::ItemName(item), code, message)
}

/// The asset constants of the program by kind: the values of the asset types.
pub(super) fn assets(cx: StoryCx<'_>) -> FxHashMap<AssetKind, Vec<ConstValue>> {
    let consts = &const_eval(cx.db, cx.program).consts;
    let mut out: FxHashMap<AssetKind, Vec<ConstValue>> = FxHashMap::default();
    let mut seen = FxHashSet::default();
    for &(file, _) in &def_maps(cx.db, cx.program).files {
        for index in 0..item_tree(cx.db, file).items.len() {
            let Some(index) = ItemIndex::from_usize(index) else {
                continue;
            };
            if let Some(value @ ConstValue::Asset { kind, .. }) =
                consts.get(&ItemId { file, index })
                && seen.insert(value.clone())
            {
                out.entry(*kind).or_default().push(value.clone());
            }
        }
    }
    out
}

/// The named non-generic functions of the program: the values of function pointer types.
pub(super) fn functions<'db>(cx: StoryCx<'db>) -> Vec<(ItemId, &'db FnSig)> {
    let mut out = Vec::new();
    for &(file, _) in &def_maps(cx.db, cx.program).files {
        for index in 0..item_tree(cx.db, file).items.len() {
            let Some(index) = ItemIndex::from_usize(index) else {
                continue;
            };
            let item = ItemId { file, index };
            if let Some(ItemSig::Fn(sig)) = cx.sigs.get(item)
                && sig.generics.params.is_empty()
                && sig.generics.parent_count == 0
            {
                out.push((item, sig));
            }
        }
    }
    out
}
