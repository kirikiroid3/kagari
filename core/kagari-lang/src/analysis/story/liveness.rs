//! What a scene keeps across each suspension: the locals a story state is made of.
//!
//! A suspension keeps what lives after it, and what the step or call itself consumes:
//! states that differ in the step shown or the scene called are different states. The
//! `&mut` parameters of the scene are kept everywhere: their final values are part of every
//! exit.

use rustc_hash::FxHashMap;

use crate::db::Db;
use crate::input::Program;
use crate::item_tree::ItemId;
use crate::mir::liveness::{block_liveness, read_uses};
use crate::mir::{
    self, BasicBlock, Body, Callee, Local, LocalSet, Operand, Rvalue, StatementKind, Terminator,
    item_mir,
};
use crate::ty::TyKind;

/// The kept locals of every suspension block of scene `item`'s body.
#[salsa::tracked(returns(ref))]
pub(super) fn scene_kept(
    db: &dyn Db,
    program: Program,
    item: ItemId,
) -> Option<FxHashMap<BasicBlock, LocalSet>> {
    let body = &item_mir(db, program, item).as_ref()?.main;
    let liveness = block_liveness(body);
    let out_params: Vec<Local> = ref_params(body)
        .filter(|(_, mutable)| *mutable)
        .map(|(local, _)| local)
        .collect();
    let mut kept = FxHashMap::default();
    for (block, data) in body.blocks.iter() {
        let (dest, next) = match &data.term {
            Terminator::Step { dest, next, .. }
            | Terminator::Rng { dest, next, .. }
            | Terminator::SceneCall { dest, next, .. } => (dest, *next),
            _ => continue,
        };
        let mut live = liveness
            .get(next)
            .map(|(live_in, _)| live_in.clone())
            .unwrap_or_default();
        match dest.as_local() {
            Some(local) => live.remove(local),
            None => read_uses(dest, &mut live),
        }
        match &data.term {
            Terminator::Step { arg, .. } => operand_locals(arg, &mut live),
            Terminator::SceneCall { callee, args, .. } => {
                if let Callee::Value(operand) = callee {
                    operand_locals(operand, &mut live);
                }
                for arg in args {
                    match ref_arg(body, arg) {
                        // The reference itself is not kept: its value is copied in. The
                        // place it points to is kept: it decides the value copied in, and a
                        // `&mut` writes the result back to it.
                        Some(target) => read_uses(&target.place, &mut live),
                        None => operand_locals(arg, &mut live),
                    }
                }
            }
            _ => {}
        }
        for local in &out_params {
            live.insert(*local);
        }
        kept.insert(block, live);
    }
    Some(kept)
}

fn operand_locals(operand: &Operand, live: &mut LocalSet) {
    if let Operand::Copy(place) = operand {
        read_uses(place, live);
    }
}

/// The parameters of `body` passed by reference, with whether the reference is `&mut`.
pub(super) fn ref_params(body: &Body) -> impl Iterator<Item = (Local, bool)> + '_ {
    body.args()
        .filter_map(|local| match body.local_decl(local)?.ty.kind() {
            TyKind::Ref { mutable, .. } => Some((local, *mutable)),
            _ => None,
        })
}

/// A reference argument of a scene call: the place it points to and whether it is `&mut`.
pub(super) struct RefArg {
    pub place: mir::Place,
    pub mutable: bool,
}

/// The place a reference-typed scene call argument points to: the place of the `&`/`&mut`
/// that defines the argument's temporary, else the place behind the reference local.
pub(super) fn ref_arg(body: &Body, arg: &Operand) -> Option<RefArg> {
    let Operand::Copy(place) = arg else {
        return None;
    };
    let local = place.as_local()?;
    let TyKind::Ref { mutable, .. } = body.local_decl(local)?.ty.kind() else {
        return None;
    };
    Some(RefArg {
        place: borrowed_place(body, local)
            .unwrap_or_else(|| place.clone().project(mir::Proj::Deref)),
        mutable: *mutable,
    })
}

/// The place `local` borrows when its only definition is a `&`/`&mut` of it.
fn borrowed_place(body: &Body, local: Local) -> Option<mir::Place> {
    let mut found = None;
    for (_, data) in body.blocks.iter() {
        for stmt in &data.stmts {
            let dest = match &stmt.kind {
                StatementKind::Assign(dest, _) | StatementKind::Call { dest, .. } => dest,
            };
            if dest.as_local() != Some(local) {
                continue;
            }
            match (&stmt.kind, found) {
                (StatementKind::Assign(_, Rvalue::Ref { place, .. }), None) => {
                    found = Some(place);
                }
                _ => return None,
            }
        }
        if let Terminator::Step { dest, .. }
        | Terminator::Rng { dest, .. }
        | Terminator::SceneCall { dest, .. } = &data.term
            && dest.as_local() == Some(local)
        {
            return None;
        }
    }
    let place = found?;
    // Writing back through a place based on the reference itself would need the reference.
    (place.base_local() != Some(local)).then(|| place.clone())
}
