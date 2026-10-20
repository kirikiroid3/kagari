//! The missing-return check over the narrative IR: a scene that declares `returns` must leave
//! through a `<return>` with a value on every path, never by reaching its end.
//!
//! Like the termination check, it is path-insensitive: every branch arm counts as feasible.

use crate::db::Db;
use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::input::Program;
use crate::ir::{IrScene, NodeId, Op, narrative_ir};
use crate::kis::game_hir;

#[salsa::tracked(returns(ref))]
pub fn missing_returns(db: &dyn Db, program: Program) -> Vec<Diagnostic> {
    let ir = narrative_ir(db, program);
    let hir = game_hir(db, program);
    let mut diagnostics = Vec::new();
    for scene in ir.scenes.iter().filter(|scene| scene.returns.is_some()) {
        let Some(last) = fall_off(scene) else {
            continue;
        };
        let decl = &hir.scenes[scene.decl];
        let mut diag = decl.span.diagnostic(
            db,
            DiagnosticCode::KisMissingReturn,
            format!(
                "scene `{}` declares `returns`, but can reach its end without `<return>`",
                decl.name
            ),
        );
        if let Some(node) = last.and_then(|n| scene.nodes.get(index(n))) {
            diag.related.push(
                node.span
                    .related(db, "the scene can reach its end after this"),
            );
        }
        diagnostics.push(diag);
    }
    diagnostics
}

/// Whether the scene's exit is reachable from its entry other than through a `Return`.
/// `Some(Some(n))` names the first node found that continues to the exit; `Some(None)` means
/// the body is empty.
fn fall_off(scene: &IrScene) -> Option<Option<NodeId>> {
    if scene.entry == scene.exit {
        return Some(None);
    }
    let mut seen = vec![false; scene.nodes.len()];
    let mut stack = vec![scene.entry];
    while let Some(id) = stack.pop() {
        let Some(slot) = seen.get_mut(index(id)) else {
            continue;
        };
        if std::mem::replace(slot, true) {
            continue;
        }
        let Some(node) = scene.nodes.get(index(id)) else {
            continue;
        };
        for next in successors(&node.op) {
            if next == scene.exit {
                return Some(Some(id));
            }
            stack.push(next);
        }
    }
    None
}

/// Control-flow successors within the scene. A `Return` has none here: it is the way out
/// that this check looks for.
fn successors(op: &Op) -> Vec<NodeId> {
    match op {
        Op::Return { .. } | Op::Exit => Vec::new(),
        Op::Say { next, .. }
        | Op::Assign { next, .. }
        | Op::Add { next, .. }
        | Op::Sub { next, .. }
        | Op::Call { next, .. }
        | Op::MetaWrite { next }
        | Op::Jump { next, .. } => vec![*next],
        Op::Choice { arms, .. } => arms.iter().map(|arm| arm.target).collect(),
        Op::Branch { arms, .. } => arms.iter().map(|arm| arm.target).collect(),
    }
}

fn index(id: NodeId) -> usize {
    usize::try_from(id.0).unwrap_or(usize::MAX)
}
