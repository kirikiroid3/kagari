//! The finished CFG: blocks nothing reaches are removed, a `Goto` to a block with no other
//! predecessor is merged with it, the blocks are renumbered, and the locals that need a
//! heap cell are found.

use la_arena::{Arena, RawIdx};

use crate::mir::{
    BasicBlock, BlockData, LocalSet, LoopInfo, Place, Proj, Rvalue, StatementKind, Terminator,
};

fn index(block: BasicBlock) -> usize {
    block.into_raw().into_u32() as usize
}

fn block_at(index: usize) -> BasicBlock {
    BasicBlock::from_raw(RawIdx::from_u32(u32::try_from(index).unwrap_or(u32::MAX)))
}

/// Applies `f` to every block `term` may continue at.
fn map_targets(term: &mut Terminator, f: impl Fn(BasicBlock) -> BasicBlock) {
    match term {
        Terminator::Goto(target) => *target = f(*target),
        Terminator::SwitchBool { then, other, .. } => {
            *then = f(*then);
            *other = f(*other);
        }
        Terminator::SwitchVariant {
            targets, otherwise, ..
        } => {
            for (_, target) in targets.iter_mut() {
                *target = f(*target);
            }
            if let Some(otherwise) = otherwise {
                *otherwise = f(*otherwise);
            }
        }
        Terminator::Return | Terminator::Unreachable => {}
        Terminator::Step { next, .. }
        | Terminator::Rng { next, .. }
        | Terminator::SceneCall { next, .. } => *next = f(*next),
    }
}

/// Threads jumps through empty `Goto` blocks, removes unreachable blocks (block 0 is the
/// entry), merges straight-line `Goto`s that stay inside the same loops, and renumbers the
/// blocks and the loops' regions. Loop headers are kept.
pub(super) fn simplify(
    mut blocks: Vec<BlockData>,
    loops: Vec<LoopInfo>,
) -> (Vec<BlockData>, Vec<LoopInfo>) {
    let count = blocks.len();
    let is_header = |block: usize| loops.iter().any(|info| index(info.header) == block);
    // Where a jump to each block may go instead: past empty `Goto` blocks.
    let forward: Vec<Option<BasicBlock>> = blocks
        .iter()
        .enumerate()
        .map(|(block, data)| match data.term {
            Terminator::Goto(target)
                if block != 0 && data.stmts.is_empty() && !is_header(block) =>
            {
                Some(target)
            }
            _ => None,
        })
        .collect();
    let thread = |mut block: BasicBlock| {
        // At most `count` steps: a cycle of empty blocks stays where it is.
        for _ in 0..count {
            match forward.get(index(block)).copied().flatten() {
                Some(next) => block = next,
                None => break,
            }
        }
        block
    };
    for data in &mut blocks {
        map_targets(&mut data.term, thread);
    }
    let mut alive = vec![false; count];
    let mut stack = vec![0usize];
    while let Some(block) = stack.pop() {
        match alive.get_mut(block) {
            Some(seen) if !*seen => *seen = true,
            _ => continue,
        }
        if let Some(data) = blocks.get(block) {
            stack.extend(data.term.successors().into_iter().map(index));
        }
    }
    let mut preds = vec![0u32; count];
    for (data, _) in blocks.iter().zip(&alive).filter(|(_, alive)| **alive) {
        for succ in data.term.successors() {
            if let Some(slot) = preds.get_mut(index(succ)) {
                *slot = slot.saturating_add(1);
            }
        }
    }
    // Loop membership of each block: a merge must not move code into or out of a loop.
    let regions: Vec<Vec<bool>> = loops
        .iter()
        .map(|info| {
            let mut member = vec![false; count];
            for block in &info.region {
                if let Some(slot) = member.get_mut(index(*block)) {
                    *slot = true;
                }
            }
            member
        })
        .collect();
    let same_loops =
        |a: usize, b: usize| regions.iter().all(|member| member.get(a) == member.get(b));
    for a in 0..count {
        if !alive.get(a).copied().unwrap_or(false) {
            continue;
        }
        while let Some(Terminator::Goto(target)) = blocks.get(a).map(|data| &data.term) {
            let b = index(*target);
            let mergeable = b != a
                && b != 0
                && preds.get(b) == Some(&1)
                && alive.get(b) == Some(&true)
                && !is_header(b)
                && same_loops(a, b);
            if !mergeable {
                break;
            }
            let Some(taken) = blocks.get_mut(b).map(|data| {
                std::mem::replace(
                    data,
                    BlockData {
                        stmts: Vec::new(),
                        term: Terminator::Unreachable,
                        term_src: None,
                    },
                )
            }) else {
                break;
            };
            if let Some(slot) = alive.get_mut(b) {
                *slot = false;
            }
            if let Some(data) = blocks.get_mut(a) {
                data.stmts.extend(taken.stmts);
                data.term = taken.term;
                data.term_src = taken.term_src;
            }
        }
    }
    // Renumber in reverse postorder from the entry (every block left is reachable), a
    // block's first successor right after it.
    let order = reverse_postorder(&blocks, &alive);
    let mut renumber: Vec<Option<BasicBlock>> = vec![None; count];
    for (position, &block) in order.iter().enumerate() {
        if let Some(slot) = renumber.get_mut(block) {
            *slot = Some(block_at(position));
        }
    }
    let map = |block: BasicBlock| {
        renumber
            .get(index(block))
            .copied()
            .flatten()
            .unwrap_or(block)
    };
    let mut slots: Vec<Option<BlockData>> = blocks.into_iter().map(Some).collect();
    let mut out = Vec::with_capacity(order.len());
    for &block in &order {
        if let Some(mut data) = slots.get_mut(block).and_then(Option::take) {
            map_targets(&mut data.term, map);
            out.push(data);
        }
    }
    let loops = loops
        .into_iter()
        .filter_map(|info| {
            let header = renumber.get(index(info.header)).copied().flatten()?;
            let mut region: Vec<BasicBlock> = info
                .region
                .iter()
                .filter_map(|block| renumber.get(index(*block)).copied().flatten())
                .collect();
            region.sort();
            region.dedup();
            Some(LoopInfo {
                src: info.src,
                header,
                region,
            })
        })
        .collect();
    (out, loops)
}

/// The `alive` blocks reachable from block 0 in reverse postorder; successors are visited
/// last-first, so a block's first successor follows it when it can.
fn reverse_postorder(blocks: &[BlockData], alive: &[bool]) -> Vec<usize> {
    let mut visited = vec![false; blocks.len()];
    let mut post = Vec::with_capacity(blocks.len());
    // `(block, its successors in visiting order, how many were visited)`.
    let mut stack: Vec<(usize, Vec<usize>, usize)> = Vec::new();
    let successors = |block: usize| -> Vec<usize> {
        blocks.get(block).map_or_else(Vec::new, |data| {
            data.term
                .successors()
                .into_iter()
                .rev()
                .map(index)
                .collect()
        })
    };
    if alive.first() == Some(&true)
        && let Some(slot) = visited.first_mut()
    {
        *slot = true;
        stack.push((0, successors(0), 0));
    }
    while let Some((block, succs, next)) = stack.last_mut() {
        match succs.get(*next).copied() {
            Some(succ) => {
                *next = next.saturating_add(1);
                let fresh = alive.get(succ) == Some(&true) && visited.get(succ) == Some(&false);
                if fresh {
                    if let Some(slot) = visited.get_mut(succ) {
                        *slot = true;
                    }
                    stack.push((succ, successors(succ), 0));
                }
            }
            None => {
                post.push(*block);
                stack.pop();
            }
        }
    }
    post.reverse();
    post
}

/// The locals whose storage is borrowed (`Ref` of a place rooted at the local without a
/// dereference) or written through a projection that does not dereference.
pub(super) fn needs_cell(blocks: &Arena<BlockData>) -> LocalSet {
    let mut set = LocalSet::default();
    let own_storage = |place: &Place| {
        if place.proj.contains(&Proj::Deref) {
            None
        } else {
            place.base_local()
        }
    };
    let written = |set: &mut LocalSet, place: &Place| {
        if !place.proj.is_empty()
            && let Some(local) = own_storage(place)
        {
            set.insert(local);
        }
    };
    for (_, block) in blocks.iter() {
        for stmt in &block.stmts {
            match &stmt.kind {
                StatementKind::Assign(place, rvalue) => {
                    written(&mut set, place);
                    if let Rvalue::Ref { place, .. } = rvalue
                        && let Some(local) = own_storage(place)
                    {
                        set.insert(local);
                    }
                }
                StatementKind::Call { dest, .. } => written(&mut set, dest),
            }
        }
        match &block.term {
            Terminator::Step { dest, .. }
            | Terminator::Rng { dest, .. }
            | Terminator::SceneCall { dest, .. } => written(&mut set, dest),
            _ => {}
        }
    }
    set
}
