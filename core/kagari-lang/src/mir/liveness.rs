//! Live locals at block boundaries: a backward data-flow over the CFG.
//!
//! Uses are every operand, the index locals of every place and the base local of every read
//! place. An `Assign` (or a call/suspension `dest`) to a bare local defines it; a write
//! through a projection uses the base. `Return` uses `_0`.

use la_arena::ArenaMap;

use super::{
    BasicBlock, Body, Callee, Local, LocalSet, Operand, Place, Proj, Rvalue, StatementKind,
    Terminator,
};

/// The `(live-in, live-out)` sets of every block.
pub fn block_liveness(body: &Body) -> ArenaMap<BasicBlock, (LocalSet, LocalSet)> {
    let mut preds: ArenaMap<BasicBlock, Vec<BasicBlock>> = ArenaMap::default();
    for (block, data) in body.blocks.iter() {
        preds.entry(block).or_default();
        for succ in data.term.successors() {
            preds.entry(succ).or_default().push(block);
        }
    }
    let mut sets: ArenaMap<BasicBlock, (LocalSet, LocalSet)> = ArenaMap::default();
    for (block, _) in body.blocks.iter() {
        sets.insert(block, (LocalSet::default(), LocalSet::default()));
    }
    // Reverse order converges faster for a backward problem.
    let mut worklist: Vec<BasicBlock> = body.blocks.iter().map(|(block, _)| block).collect();
    let mut queued = Queued::full(&worklist);
    while let Some(block) = worklist.pop() {
        queued.remove(block);
        let Some(data) = body.block(block) else {
            continue;
        };
        let mut out = LocalSet::default();
        for succ in data.term.successors() {
            if let Some((live_in, _)) = sets.get(succ) {
                out.union(live_in);
            }
        }
        let live_in = transfer(body, block, out.clone());
        let changed = sets.get(block).is_none_or(|(old, _)| *old != live_in);
        sets.insert(block, (live_in, out));
        if changed {
            for pred in preds.get(block).into_iter().flatten() {
                if queued.insert(*pred) {
                    worklist.push(*pred);
                }
            }
        }
    }
    sets
}

/// The live-in set of `block` given its live-out set.
fn transfer(body: &Body, block: BasicBlock, mut live: LocalSet) -> LocalSet {
    let Some(data) = body.block(block) else {
        return live;
    };
    let mut uses = LocalSet::default();
    match &data.term {
        Terminator::Goto(_) | Terminator::Unreachable => {}
        Terminator::SwitchBool { cond, .. } => operand_uses(cond, &mut uses),
        Terminator::SwitchVariant { place, .. } => read_uses(place, &mut uses),
        Terminator::Return => {
            uses.insert(Body::RETURN);
        }
        Terminator::Step { arg, dest, .. } => {
            write(dest, &mut live, &mut uses);
            operand_uses(arg, &mut uses);
        }
        Terminator::Rng { dest, .. } => write(dest, &mut live, &mut uses),
        Terminator::SceneCall {
            callee, args, dest, ..
        } => {
            write(dest, &mut live, &mut uses);
            callee_uses(callee, &mut uses);
            for arg in args {
                operand_uses(arg, &mut uses);
            }
        }
    }
    live.union(&uses);
    for stmt in data.stmts.iter().rev() {
        let mut uses = LocalSet::default();
        match &stmt.kind {
            StatementKind::Assign(place, rvalue) => {
                write(place, &mut live, &mut uses);
                rvalue_uses(rvalue, &mut uses);
            }
            StatementKind::Call { dest, callee, args } => {
                write(dest, &mut live, &mut uses);
                callee_uses(callee, &mut uses);
                for arg in args {
                    operand_uses(arg, &mut uses);
                }
            }
        }
        live.union(&uses);
    }
    live
}

/// A write to `place`: kills a bare local, otherwise uses the base and index locals.
fn write(place: &Place, live: &mut LocalSet, uses: &mut LocalSet) {
    match place.as_local() {
        Some(local) => live.remove(local),
        None => read_uses(place, uses),
    }
}

/// The locals reading `place` uses: its base and its index locals.
pub(crate) fn read_uses(place: &Place, uses: &mut LocalSet) {
    if let Some(local) = place.base_local() {
        uses.insert(local);
    }
    for local in index_locals(place) {
        uses.insert(local);
    }
}

pub(crate) fn index_locals(place: &Place) -> impl Iterator<Item = Local> + '_ {
    place.proj.iter().filter_map(|proj| match proj {
        Proj::Index(local) => Some(*local),
        Proj::Deref | Proj::Field(_) | Proj::Downcast(_) => None,
    })
}

pub(crate) fn operand_uses(operand: &Operand, uses: &mut LocalSet) {
    match operand {
        Operand::Copy(place) => read_uses(place, uses),
        Operand::Const(_) => {}
    }
}

pub(crate) fn callee_uses(callee: &Callee, uses: &mut LocalSet) {
    match callee {
        Callee::Value(operand) => operand_uses(operand, uses),
        Callee::Fn(_) | Callee::Builtin(..) => {}
    }
}

pub(crate) fn rvalue_uses(rvalue: &Rvalue, uses: &mut LocalSet) {
    match rvalue {
        Rvalue::Use(operand)
        | Rvalue::IntNeg { operand, .. }
        | Rvalue::IntNot { operand, .. }
        | Rvalue::IntClamp { operand, .. }
        | Rvalue::BoolNot(operand)
        | Rvalue::DecNeg(operand)
        | Rvalue::RatNeg(operand)
        | Rvalue::Repeat { operand, .. }
        | Rvalue::Cast { operand, .. } => operand_uses(operand, uses),
        Rvalue::IntBin { lhs, rhs, .. }
        | Rvalue::BoolBin { lhs, rhs, .. }
        | Rvalue::DecBin { lhs, rhs, .. }
        | Rvalue::RatBin { lhs, rhs, .. }
        | Rvalue::Cmp { lhs, rhs, .. } => {
            operand_uses(lhs, uses);
            operand_uses(rhs, uses);
        }
        Rvalue::Aggregate { ops, .. } | Rvalue::Closure { captures: ops, .. } => {
            for operand in ops {
                operand_uses(operand, uses);
            }
        }
        Rvalue::Ref { place, .. } | Rvalue::Len(place) => read_uses(place, uses),
    }
}

/// Which blocks are on the worklist.
struct Queued(Vec<bool>);

impl Queued {
    fn full(blocks: &[BasicBlock]) -> Self {
        let len = blocks
            .iter()
            .map(|block| block.into_raw().into_u32() as usize)
            .max()
            .map_or(0, |max| max.saturating_add(1));
        Queued(vec![true; len])
    }

    fn remove(&mut self, block: BasicBlock) {
        if let Some(slot) = self.0.get_mut(block.into_raw().into_u32() as usize) {
            *slot = false;
        }
    }

    /// Marks `block` queued; returns whether it was not.
    fn insert(&mut self, block: BasicBlock) -> bool {
        match self.0.get_mut(block.into_raw().into_u32() as usize) {
            Some(slot) => !std::mem::replace(slot, true),
            None => false,
        }
    }
}
