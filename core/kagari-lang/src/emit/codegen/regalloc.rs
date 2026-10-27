//! Register allocation for one MIR body.
//!
//! - The arguments (locals `1..=arg_count`) are `r0..`, fixed and never reused.
//! - Every celled local (`Body::needs_cell`) gets a dedicated register holding the reference
//!   to its heap cell.
//! - The other locals share registers by linear scan over live intervals: an interval covers
//!   every position where the local is live (block boundaries from
//!   [`block_liveness`](crate::mir::liveness::block_liveness), plus its uses and definitions),
//!   with no holes. A local that is never read gets no register.
//! - Above them is one argument window, sized to the largest call, aggregate or closure, and
//!   above that the scratch pool.
//!
//! Within a statement every read happens before the definition is written, so an interval
//! ending at the uses of a statement and one starting at its definition may share a register.

use la_arena::ArenaMap;

use crate::mir::liveness::{block_liveness, callee_uses, operand_uses, read_uses, rvalue_uses};
use crate::mir::{
    BasicBlock, Body, Local, LocalSet, Operand, Place, Rvalue, StatementKind, Terminator,
};

use super::R;

/// Where a local lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Loc {
    /// In this register.
    Reg(R),
    /// In a heap cell; the register holds the reference to it.
    Cell(R),
    /// Never read: a write to it goes to a scratch register.
    Dead,
}

/// The register layout of a body.
pub(super) struct Frame {
    /// Per local, by index.
    pub locs: Vec<Loc>,
    /// The cells to create on entry: the cell register and the argument register holding
    /// its initial value (`None` for a local that starts uninitialized).
    pub cells: Vec<(R, Option<R>)>,
    /// The first register of the argument window.
    pub window: R,
    /// The first scratch register.
    pub scratch: R,
}

impl Frame {
    pub fn loc(&self, local: Local) -> Loc {
        self.locs
            .get(local.into_raw().into_u32() as usize)
            .copied()
            .unwrap_or(Loc::Dead)
    }
}

fn r(n: usize) -> R {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The live interval of every local, in positions of `layout`.
struct Intervals {
    spans: Vec<Option<(u32, u32)>>,
    /// The locals that are read somewhere.
    used: LocalSet,
}

impl Intervals {
    fn touch(&mut self, local: Local, pos: u32) {
        let Some(slot) = self.spans.get_mut(local.into_raw().into_u32() as usize) else {
            return;
        };
        *slot = Some(match *slot {
            Some((start, end)) => (start.min(pos), end.max(pos)),
            None => (pos, pos),
        });
    }

    fn touch_all(&mut self, set: &LocalSet, pos: u32) {
        for local in set.iter() {
            self.touch(local, pos);
        }
    }
}

/// The locals a write to `place` reads (its base and index locals, unless it is a bare
/// local), and the local it defines.
fn write_effects(place: &Place, uses: &mut LocalSet) -> Option<Local> {
    match place.as_local() {
        Some(local) => Some(local),
        None => {
            read_uses(place, uses);
            None
        }
    }
}

fn intervals(body: &Body, layout: &[BasicBlock]) -> Intervals {
    let live = block_liveness(body);
    let mut out = Intervals {
        spans: vec![None; body.locals.len()],
        used: LocalSet::default(),
    };
    let mut pos: u32 = 0;
    let next = |pos: &mut u32| {
        *pos = pos.saturating_add(1);
        *pos
    };
    for block in layout {
        let Some(data) = body.block(*block) else {
            continue;
        };
        let (live_in, live_out) = live.get(*block).cloned().unwrap_or_default();
        out.touch_all(&live_in, pos);
        let effects = data
            .stmts
            .iter()
            .map(|stmt| stmt_effects(&stmt.kind))
            .chain(std::iter::once(term_effects(&data.term)));
        let mut def_pos = pos;
        for effect in effects {
            let uses = effect.uses();
            let use_pos = next(&mut pos);
            out.touch_all(&uses, use_pos);
            out.used.union(&uses);
            def_pos = next(&mut pos);
            if let Some(def) = effect.def {
                out.touch(def, def_pos);
            }
        }
        out.touch_all(&live_out, def_pos);
        next(&mut pos);
    }
    out
}

/// The operands a statement moves into the argument window.
fn window_ops(stmt: &StatementKind) -> &[Operand] {
    match stmt {
        StatementKind::Call { args, .. } => args,
        StatementKind::Assign(_, Rvalue::Aggregate { ops, .. })
        | StatementKind::Assign(_, Rvalue::Closure { captures: ops, .. }) => ops,
        StatementKind::Assign(..) => &[],
    }
}

/// The largest number of operands moved into the argument window at once.
fn window_size(body: &Body) -> usize {
    let mut size = 0;
    for (_, data) in body.blocks.iter() {
        for stmt in &data.stmts {
            size = size.max(window_ops(&stmt.kind).len());
        }
        if let Terminator::SceneCall { args, .. } = &data.term {
            size = size.max(args.len());
        }
    }
    size
}

/// What one statement or terminator reads and defines.
struct Effects<'a> {
    /// The operands it moves into the argument window.
    window: &'a [Operand],
    /// The locals it reads other than through `window`.
    other: LocalSet,
    /// The bare local it writes.
    def: Option<Local>,
}

impl Effects<'_> {
    /// Every local it reads.
    fn uses(&self) -> LocalSet {
        let mut uses = self.other.clone();
        for operand in self.window {
            operand_uses(operand, &mut uses);
        }
        uses
    }
}

fn stmt_effects(stmt: &StatementKind) -> Effects<'_> {
    let mut other = LocalSet::default();
    let window = window_ops(stmt);
    let def = match stmt {
        StatementKind::Assign(place, rvalue) => {
            if window.is_empty() {
                rvalue_uses(rvalue, &mut other);
            }
            write_effects(place, &mut other)
        }
        StatementKind::Call { dest, callee, .. } => {
            callee_uses(callee, &mut other);
            write_effects(dest, &mut other)
        }
    };
    Effects { window, other, def }
}

fn term_effects(term: &Terminator) -> Effects<'_> {
    let mut other = LocalSet::default();
    let (window, def): (&[Operand], _) = match term {
        Terminator::Goto(_) | Terminator::Unreachable => (&[], None),
        Terminator::SwitchBool { cond, .. } => {
            operand_uses(cond, &mut other);
            (&[], None)
        }
        Terminator::SwitchVariant { place, .. } => {
            read_uses(place, &mut other);
            (&[], None)
        }
        Terminator::Return => {
            other.insert(Body::RETURN);
            (&[], None)
        }
        Terminator::Step { arg, dest, .. } => {
            operand_uses(arg, &mut other);
            (&[], write_effects(dest, &mut other))
        }
        Terminator::Rng { dest, .. } => (&[], write_effects(dest, &mut other)),
        Terminator::SceneCall {
            callee, args, dest, ..
        } => {
            callee_uses(callee, &mut other);
            (args.as_slice(), write_effects(dest, &mut other))
        }
    };
    Effects { window, other, def }
}

fn local_index(local: Local) -> usize {
    local.into_raw().into_u32() as usize
}

/// The temporaries computed straight into the argument window, with their window slot: a
/// local defined by one statement and read only by the next statement of its block, once,
/// as a whole window operand. Nothing writes that slot between the two.
fn forwarded(body: &Body, locs: &[Loc]) -> Vec<(usize, usize)> {
    let len = body.locals.len();
    let mut reads = vec![0u32; len];
    let mut defs = vec![0u32; len];
    let effects: Vec<Vec<Effects<'_>>> = body
        .blocks
        .iter()
        .map(|(_, data)| {
            data.stmts
                .iter()
                .map(|stmt| stmt_effects(&stmt.kind))
                .chain(std::iter::once(term_effects(&data.term)))
                .collect()
        })
        .collect();
    for effect in effects.iter().flatten() {
        for local in effect.uses().iter() {
            if let Some(count) = reads.get_mut(local_index(local)) {
                *count = count.saturating_add(1);
            }
        }
        if let Some(count) = effect
            .def
            .and_then(|local| defs.get_mut(local_index(local)))
        {
            *count = count.saturating_add(1);
        }
    }
    let mut out = Vec::new();
    for pair in effects.iter().flat_map(|block| block.windows(2)) {
        let [first, second] = pair else {
            continue;
        };
        let Some(local) = first.def else {
            continue;
        };
        let index = local_index(local);
        let single = reads.get(index) == Some(&1) && defs.get(index) == Some(&1);
        if !single || locs.get(index) != Some(&Loc::Dead) || second.other.contains(local) {
            continue;
        }
        let mut slots = second.window.iter().enumerate().filter(|(_, operand)| {
            let mut uses = LocalSet::default();
            operand_uses(operand, &mut uses);
            uses.contains(local)
        });
        if let (Some((slot, Operand::Copy(place))), None) = (slots.next(), slots.next())
            && place.as_local() == Some(local)
        {
            out.push((index, slot));
        }
    }
    out
}

/// Allocates the registers of `body`, whose blocks are emitted in the order `layout`.
pub(super) fn allocate(body: &Body, layout: &[BasicBlock]) -> Frame {
    let len = body.locals.len();
    let args = body.arg_count as usize;
    let mut locs = vec![Loc::Dead; len];
    let mut cells = Vec::new();
    let mut top = r(args);
    for (index, loc) in locs.iter_mut().enumerate() {
        let local = Local::from_raw(la_arena::RawIdx::from_u32(r(index)));
        let arg = (1..=args).contains(&index);
        let arg_reg = r(index.saturating_sub(1));
        if body.needs_cell.contains(local) {
            *loc = Loc::Cell(top);
            cells.push((top, arg.then_some(arg_reg)));
            top = top.saturating_add(1);
        } else if arg {
            *loc = Loc::Reg(arg_reg);
        }
    }
    let forward = forwarded(body, &locs);
    let spans = intervals(body, layout);
    let mut order: Vec<(u32, u32, usize)> = spans
        .spans
        .iter()
        .enumerate()
        .filter_map(|(index, span)| {
            let local = Local::from_raw(la_arena::RawIdx::from_u32(r(index)));
            let free = locs.get(index) == Some(&Loc::Dead)
                && spans.used.contains(local)
                && !forward.iter().any(|(forwarded, _)| *forwarded == index);
            let (start, end) = (*span)?;
            free.then_some((start, end, index))
        })
        .collect();
    order.sort_unstable();
    let base = top;
    // `busy[i]` is the end of the interval holding register `base + i`.
    let mut busy: Vec<Option<u32>> = Vec::new();
    for (start, end, index) in order {
        for slot in busy.iter_mut() {
            if slot.is_some_and(|until| until < start) {
                *slot = None;
            }
        }
        let position = match busy.iter().position(Option::is_none) {
            Some(position) => position,
            None => {
                busy.push(None);
                busy.len().saturating_sub(1)
            }
        };
        if let Some(slot) = busy.get_mut(position) {
            *slot = Some(end);
        }
        if let Some(loc) = locs.get_mut(index) {
            *loc = Loc::Reg(base.saturating_add(r(position)));
        }
    }
    let window = base.saturating_add(r(busy.len()));
    for (index, slot) in forward {
        if let Some(loc) = locs.get_mut(index) {
            *loc = Loc::Reg(window.saturating_add(r(slot)));
        }
    }
    let scratch = window.saturating_add(r(window_size(body)));
    Frame {
        locs,
        cells,
        window,
        scratch,
    }
}

/// The blocks of `body` reachable from its entry, in arena order (the builder leaves them in
/// reverse postorder).
pub(super) fn layout(body: &Body) -> Vec<BasicBlock> {
    let mut seen: ArenaMap<BasicBlock, ()> = ArenaMap::default();
    let mut stack = vec![body.entry];
    while let Some(block) = stack.pop() {
        if seen.contains_idx(block) {
            continue;
        }
        let Some(data) = body.block(block) else {
            continue;
        };
        seen.insert(block, ());
        stack.extend(data.term.successors());
    }
    let mut order: Vec<BasicBlock> = body
        .blocks
        .iter()
        .map(|(block, _)| block)
        .filter(|block| seen.contains_idx(*block))
        .collect();
    // The entry comes first: it follows the prologue.
    if let Some(position) = order.iter().position(|block| *block == body.entry) {
        let entry = order.remove(position);
        order.insert(0, entry);
    }
    order
}
