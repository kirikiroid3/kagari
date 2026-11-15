//! The static shape of a scene: the locals each of its segments may touch, the story
//! variables it can access (its *footprint*) and the runs of its suspensions that depend on
//! a part of the state only (its *regions*). The exploration shares states with it: a scene
//! is explored once per valuation of its footprint, a region once per valuation of its key
//! (`docs/dev/src/compiler/analysis.md`, "Sharing states").
//!
//! Every set here over-approximates what the code does. A set that is too small makes a
//! segment reach a local without a cell or a story variable its context does not hold; the
//! exploration then gives that path up (`W5007`) and never computes a wrong state.

use std::collections::VecDeque;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::analysis::influence::contains_mut_ref;
use crate::def_map::def_maps;
use crate::item_tree::{FnKind, ItemId, ItemIndex, ItemKind, item_tree};
use crate::mir::{
    BasicBlock, Body, Callee, ItemMir, Local, LocalSet, Operand, Place, PlaceBase, Proj, Rvalue,
    StatementKind, Terminator, item_mir,
};
use crate::ty::FnDefId;

use super::Scenes;
use super::explore::StoryCx;
use super::liveness::{ref_arg, ref_params};

/// Whether the exploration shares states. `Off` explores every state in its scene's own
/// context with every story variable: the reference the tests compare `On` against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Sharing {
    On,
    #[cfg_attr(not(test), allow(dead_code))]
    Off,
}

/// Story-variable slots as a bitset.
#[derive(Clone, Debug, Default)]
pub(super) struct SlotSet(Vec<u64>);

impl SlotSet {
    pub fn new() -> SlotSet {
        SlotSet(Vec::new())
    }

    fn split(slot: usize) -> (usize, usize) {
        (slot / 64, slot % 64)
    }

    pub fn insert(&mut self, slot: usize) {
        let (word, bit) = Self::split(slot);
        if self.0.len() <= word {
            self.0.resize(word.saturating_add(1), 0);
        }
        if let Some(bits) = self.0.get_mut(word) {
            *bits |= 1u64 << bit;
        }
    }

    pub fn contains(&self, slot: usize) -> bool {
        let (word, bit) = Self::split(slot);
        self.0
            .get(word)
            .is_some_and(|bits| bits & (1u64 << bit) != 0)
    }

    /// Adds every slot of `other`; returns whether `self` changed.
    pub fn union(&mut self, other: &SlotSet) -> bool {
        if self.0.len() < other.0.len() {
            self.0.resize(other.0.len(), 0);
        }
        let mut changed = false;
        for (bits, more) in self.0.iter_mut().zip(&other.0) {
            let next = *bits | more;
            changed |= next != *bits;
            *bits = next;
        }
        changed
    }

    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|bits| *bits == 0)
    }
}

/// The shape of one scene.
pub(super) struct SceneShape {
    /// Per segment start (`None`: the scene's entry), the locals the segment may touch.
    touched: FxHashMap<Option<BasicBlock>, Box<[Local]>>,
    /// The parameters passed by reference, of either mutability.
    pub by_ref: Box<[Local]>,
    /// The story variables the scene can access, whatever it calls included; `None` for
    /// every story variable (sharing is off, or the scene runs code that is not known here).
    pub footprint: Option<SlotSet>,
    /// The level-1 regions by ascending header, each followed by its level-2 regions.
    pub regions: Vec<Region>,
    /// `(enclosing region, header)` to the region's index.
    headers: FxHashMap<(Option<u32>, BasicBlock), u32>,
}

impl SceneShape {
    /// The locals a segment starting at `start` may touch; empty when unknown.
    pub fn touched(&self, start: Option<BasicBlock>) -> &[Local] {
        self.touched.get(&start).map_or(&[], |locals| &locals[..])
    }

    /// The region that starts at `block` for a context exploring `enclosing`.
    pub fn region_at(&self, enclosing: Option<u32>, block: BasicBlock) -> Option<u32> {
        self.headers.get(&(enclosing, block)).copied()
    }
}

/// A single-entry run of suspensions whose segments write no story variable and no local
/// kept at its header: every state inside it is the state it was entered with plus data of
/// its own, so it is explored once per valuation of what its segments read (its key).
pub(super) struct Region {
    /// The level-1 region a level-2 region lies in.
    #[cfg_attr(not(test), allow(dead_code))]
    pub parent: Option<u32>,
    pub header: BasicBlock,
    /// The header and the blocks it owns; for membership tests only.
    pub blocks: FxHashSet<BasicBlock>,
    /// Per kept local of the header, in ascending order: whether the region reads it.
    pub key_locals: Box<[bool]>,
    /// The story variables the region reads; none for a level-2 region.
    pub key_vars: SlotSet,
}

/// One shape per entry of `scenes.list`, in order.
pub(super) fn shapes(
    cx: StoryCx<'_>,
    scenes: &Scenes<'_>,
    vars: &[ItemId],
    sharing: Sharing,
) -> Vec<SceneShape> {
    let slots: FxHashMap<ItemId, usize> = vars
        .iter()
        .enumerate()
        .map(|(slot, var)| (*var, slot))
        .collect();
    let functions = Functions::collect(cx, &slots);
    scenes
        .list
        .iter()
        .map(|scene| {
            let body = &scene.mir.main;
            let by_ref = ref_params(body).map(|(local, _)| local).collect();
            match functions.get(scene.item) {
                Some(function) => Shaper {
                    body,
                    kept: scene.kept,
                    function,
                    functions: &functions,
                }
                .shape(by_ref, sharing),
                None => SceneShape {
                    touched: FxHashMap::default(),
                    by_ref,
                    footprint: None,
                    regions: Vec::new(),
                    headers: FxHashMap::default(),
                },
            }
        })
        .collect()
}

fn index(block: BasicBlock) -> usize {
    block.into_raw().into_u32() as usize
}

/// The story variables a piece of code may read and write; `opaque` when it may run code
/// that is not known here (a call through a pointer or a closure).
#[derive(Clone, Default)]
struct Access {
    read: SlotSet,
    written: SlotSet,
    opaque: bool,
}

impl Access {
    /// Adds `other`; returns whether `self` changed.
    fn union(&mut self, other: &Access) -> bool {
        let mut changed = self.read.union(&other.read);
        changed |= self.written.union(&other.written);
        if other.opaque && !self.opaque {
            self.opaque = true;
            changed = true;
        }
        changed
    }
}

/// The story variables and the locals some places access.
#[derive(Default)]
struct Uses {
    read: SlotSet,
    written: SlotSet,
    reads: LocalSet,
    writes: LocalSet,
}

/// The scene a `SceneCall` enters.
enum SceneCallee {
    None,
    Item(ItemId),
    /// A pointer, or anything else that does not name its scene.
    Unknown,
}

/// What one block of a `pushdown fn` accesses.
struct BlockInfo {
    stmts: Uses,
    /// The operands of its terminator.
    operands: Uses,
    /// The writes that resume its suspension: of the suspension's result to its `dest` and
    /// of a called scene's `outs` to the places the `&mut` arguments borrow.
    resume: Uses,
    /// A statement calls through a pointer or a closure.
    opaque: bool,
    /// The functions its statements call by name.
    calls: Vec<ItemId>,
    scene: SceneCallee,
}

/// The places of a body, as the MIR executor reaches them.
struct Places<'a> {
    body: &'a Body,
    slots: &'a FxHashMap<ItemId, usize>,
}

impl Places<'_> {
    fn read_local(&self, local: Local, out: &mut Uses) {
        out.reads.insert(local);
        // Whatever holds a `&mut` can be written through wherever it is read.
        if self
            .body
            .local_decl(local)
            .is_some_and(|decl| contains_mut_ref(&decl.ty))
        {
            out.writes.insert(local);
        }
    }

    fn place(&self, place: &Place, write: bool, out: &mut Uses) {
        match place.base {
            PlaceBase::StoryVar(item) => {
                if let Some(&slot) = self.slots.get(&item) {
                    // A written variable is thawed from the state first.
                    out.read.insert(slot);
                    if write {
                        out.written.insert(slot);
                    }
                }
            }
            PlaceBase::Local(local) => {
                if write {
                    out.writes.insert(local);
                    if !place.proj.is_empty() {
                        self.read_local(local, out);
                    }
                } else {
                    self.read_local(local, out);
                }
            }
        }
        for proj in &place.proj {
            match proj {
                Proj::Index(local) => self.read_local(*local, out),
                Proj::Deref | Proj::Field(_) | Proj::Downcast(_) => {}
            }
        }
    }

    fn operand(&self, operand: &Operand, out: &mut Uses) {
        match operand {
            Operand::Copy(place) => self.place(place, false, out),
            Operand::Const(_) => {}
        }
    }

    fn rvalue(&self, rvalue: &Rvalue, out: &mut Uses) {
        match rvalue {
            Rvalue::Use(operand)
            | Rvalue::IntNeg { operand, .. }
            | Rvalue::IntNot { operand, .. }
            | Rvalue::IntClamp { operand, .. }
            | Rvalue::BoolNot(operand)
            | Rvalue::DecNeg(operand)
            | Rvalue::RatNeg(operand)
            | Rvalue::Repeat { operand, .. }
            | Rvalue::Cast { operand, .. } => self.operand(operand, out),
            Rvalue::IntBin { lhs, rhs, .. }
            | Rvalue::BoolBin { lhs, rhs, .. }
            | Rvalue::DecBin { lhs, rhs, .. }
            | Rvalue::RatBin { lhs, rhs, .. }
            | Rvalue::Cmp { lhs, rhs, .. } => {
                self.operand(lhs, out);
                self.operand(rhs, out);
            }
            Rvalue::Aggregate { ops, .. } | Rvalue::Closure { captures: ops, .. } => {
                for operand in ops {
                    self.operand(operand, out);
                }
            }
            Rvalue::Ref { mutable, place } => {
                self.place(place, false, out);
                if *mutable {
                    self.place(place, true, out);
                }
            }
            Rvalue::Len(place) => self.place(place, false, out),
        }
    }

    fn block(&self, block: BasicBlock) -> BlockInfo {
        let mut info = BlockInfo {
            stmts: Uses::default(),
            operands: Uses::default(),
            resume: Uses::default(),
            opaque: false,
            calls: Vec::new(),
            scene: SceneCallee::None,
        };
        let Some(data) = self.body.block(block) else {
            return info;
        };
        for stmt in &data.stmts {
            match &stmt.kind {
                StatementKind::Assign(dest, rvalue) => {
                    self.rvalue(rvalue, &mut info.stmts);
                    self.place(dest, true, &mut info.stmts);
                }
                StatementKind::Call { dest, callee, args } => {
                    match callee {
                        Callee::Fn(fn_ref) | Callee::Builtin(_, fn_ref) => {
                            if let FnDefId::Item(item) = fn_ref.def {
                                info.calls.push(item);
                            }
                        }
                        Callee::Value(operand) => {
                            self.operand(operand, &mut info.stmts);
                            info.opaque = true;
                        }
                    }
                    for arg in args {
                        self.operand(arg, &mut info.stmts);
                    }
                    self.place(dest, true, &mut info.stmts);
                }
            }
        }
        match &data.term {
            Terminator::Goto(_) | Terminator::Return | Terminator::Unreachable => {}
            Terminator::SwitchBool { cond, .. } => self.operand(cond, &mut info.operands),
            Terminator::SwitchVariant { place, .. } => {
                self.place(place, false, &mut info.operands);
            }
            Terminator::Step { arg, dest, .. } => {
                self.operand(arg, &mut info.operands);
                self.place(dest, true, &mut info.resume);
            }
            Terminator::Rng { dest, .. } => self.place(dest, true, &mut info.resume),
            Terminator::SceneCall {
                callee, args, dest, ..
            } => {
                info.scene = match callee {
                    Callee::Fn(fn_ref) => match fn_ref.def {
                        FnDefId::Item(item) => SceneCallee::Item(item),
                        FnDefId::StructCtor(_) | FnDefId::VariantCtor(..) => SceneCallee::Unknown,
                    },
                    Callee::Builtin(..) => SceneCallee::Unknown,
                    Callee::Value(operand) => {
                        self.operand(operand, &mut info.operands);
                        SceneCallee::Unknown
                    }
                };
                for arg in args {
                    self.operand(arg, &mut info.operands);
                    if let Some(target) = ref_arg(self.body, arg)
                        && target.mutable
                    {
                        self.place(&target.place, true, &mut info.resume);
                    }
                }
                self.place(dest, true, &mut info.resume);
            }
        }
        info
    }
}

/// A `pushdown fn` with a body: its blocks (by block index) and the story variables it can
/// access, whatever it calls included.
struct Function {
    blocks: Vec<BlockInfo>,
    total: Access,
}

impl Function {
    fn scan(mir: &ItemMir, slots: &FxHashMap<ItemId, usize>) -> Function {
        let places = Places {
            body: &mir.main,
            slots,
        };
        let mut blocks: Vec<BlockInfo> = mir
            .main
            .blocks
            .iter()
            .map(|(block, _)| places.block(block))
            .collect();
        // A closure that mentions a story variable may be called from anywhere the value
        // gets to.
        let closures = mir.closures.iter().any(|(_, body)| {
            let places = Places { body, slots };
            body.blocks.iter().any(|(block, _)| {
                let info = places.block(block);
                [&info.stmts, &info.operands, &info.resume]
                    .iter()
                    .any(|uses| !uses.read.is_empty())
            })
        });
        let mut total = Access::default();
        for info in &mut blocks {
            info.opaque |= closures;
            total.opaque |= info.opaque;
            for uses in [&info.stmts, &info.operands, &info.resume] {
                total.read.union(&uses.read);
                total.written.union(&uses.written);
            }
        }
        Function { blocks, total }
    }
}

/// Every `pushdown fn` of the program that has a body.
struct Functions {
    index: FxHashMap<ItemId, usize>,
    list: Vec<Function>,
}

impl Functions {
    fn collect(cx: StoryCx<'_>, slots: &FxHashMap<ItemId, usize>) -> Functions {
        let mut out = Functions {
            index: FxHashMap::default(),
            list: Vec::new(),
        };
        for &(file, _) in &def_maps(cx.db, cx.program).files {
            let tree = item_tree(cx.db, file);
            for (index, data) in tree.items.iter().enumerate() {
                let Some(index) = ItemIndex::from_usize(index) else {
                    continue;
                };
                if !matches!(
                    data.kind,
                    ItemKind::Fn {
                        kind: FnKind::Pushdown,
                        has_body: true,
                        ..
                    }
                ) {
                    continue;
                }
                let item = ItemId { file, index };
                let Some(mir) = item_mir(cx.db, cx.program, item).as_ref() else {
                    continue;
                };
                out.index.insert(item, out.list.len());
                out.list.push(Function::scan(mir, slots));
            }
        }
        out.close();
        out
    }

    fn get(&self, item: ItemId) -> Option<&Function> {
        self.list.get(*self.index.get(&item)?)
    }

    /// Adds to every total the totals of the functions and scenes the function calls, until
    /// nothing changes.
    fn close(&mut self) {
        let mut edges: Vec<Vec<usize>> = Vec::with_capacity(self.list.len());
        for function in &mut self.list {
            let mut callees = Vec::new();
            for info in &function.blocks {
                callees.extend(
                    info.calls
                        .iter()
                        .filter_map(|item| self.index.get(item).copied()),
                );
                match info.scene {
                    SceneCallee::None => {}
                    SceneCallee::Item(item) => match self.index.get(&item) {
                        Some(callee) => callees.push(*callee),
                        None => function.total.opaque = true,
                    },
                    SceneCallee::Unknown => function.total.opaque = true,
                }
            }
            edges.push(callees);
        }
        loop {
            let mut changed = false;
            for (caller, callees) in edges.iter().enumerate() {
                for callee in callees {
                    let Some(total) = self.list.get(*callee).map(|f| f.total.clone()) else {
                        continue;
                    };
                    if let Some(function) = self.list.get_mut(caller) {
                        changed |= function.total.union(&total);
                    }
                }
            }
            if !changed {
                return;
            }
        }
    }
}

/// What the segment that starts after one suspension (or at the scene's entry) may do, on
/// any path to the suspensions or the `Return` it can stop at.
#[derive(Default)]
struct Segment {
    /// The suspension blocks it can stop at, ascending.
    stops: Vec<BasicBlock>,
    returns: bool,
    access: Access,
    reads: LocalSet,
    writes: LocalSet,
}

struct Shaper<'a> {
    body: &'a Body,
    kept: &'a FxHashMap<BasicBlock, LocalSet>,
    function: &'a Function,
    functions: &'a Functions,
}

impl Shaper<'_> {
    fn info(&self, block: BasicBlock) -> Option<&BlockInfo> {
        self.function.blocks.get(index(block))
    }

    fn shape(&self, by_ref: Box<[Local]>, sharing: Sharing) -> SceneShape {
        let mut touched = FxHashMap::default();
        let (entry, locals) = self.segment(None);
        touched.insert(None, locals);
        let suspensions: Vec<BasicBlock> = self
            .body
            .blocks
            .iter()
            .filter(|(_, data)| data.term.is_suspension())
            .map(|(block, _)| block)
            .collect();
        let mut segments = FxHashMap::default();
        for &block in &suspensions {
            let (segment, locals) = self.segment(Some(block));
            touched.insert(Some(block), locals);
            segments.insert(block, segment);
        }
        let mut shape = SceneShape {
            touched,
            by_ref,
            footprint: None,
            regions: Vec::new(),
            headers: FxHashMap::default(),
        };
        if sharing == Sharing::On {
            let total = &self.function.total;
            if !total.opaque {
                let mut footprint = total.read.clone();
                footprint.union(&total.written);
                shape.footprint = Some(footprint);
            }
            self.regions(&entry, &suspensions, &segments, &mut shape);
        }
        shape
    }

    /// The segment starting at `start` and the locals it may touch, ascending.
    fn segment(&self, start: Option<BasicBlock>) -> (Segment, Box<[Local]>) {
        let mut segment = Segment::default();
        let mut touched = LocalSet::default();
        touched.insert(Body::RETURN);
        for arg in self.body.args() {
            touched.insert(arg);
        }
        let first = match start {
            None => Some(self.body.entry),
            Some(block) => {
                if let Some(info) = self.info(block) {
                    segment.access.read.union(&info.resume.read);
                    segment.access.written.union(&info.resume.written);
                    segment.reads.union(&info.resume.reads);
                    segment.writes.union(&info.resume.writes);
                    // A segment that starts at this block evaluates its suspension.
                    touched.union(&info.operands.reads);
                    touched.union(&info.operands.writes);
                }
                if let Some(kept) = self.kept.get(&block) {
                    touched.union(kept);
                }
                match self.body.block(block).map(|data| &data.term) {
                    Some(Terminator::Step { next, .. } | Terminator::Rng { next, .. }) => {
                        Some(*next)
                    }
                    Some(Terminator::SceneCall { next, .. }) => {
                        // The callee ran in between.
                        segment.access.opaque = true;
                        Some(*next)
                    }
                    _ => None,
                }
            }
        };
        let mut seen = FxHashSet::default();
        let mut queue = VecDeque::new();
        if let Some(first) = first {
            seen.insert(first);
            queue.push_back(first);
        }
        while let Some(block) = queue.pop_front() {
            let (Some(data), Some(info)) = (self.body.block(block), self.info(block)) else {
                continue;
            };
            segment.access.read.union(&info.stmts.read);
            segment.access.written.union(&info.stmts.written);
            segment.access.read.union(&info.operands.read);
            segment.access.opaque |= info.opaque;
            for call in &info.calls {
                if let Some(callee) = self.functions.get(*call) {
                    segment.access.union(&callee.total);
                }
            }
            for uses in [&info.stmts, &info.operands] {
                segment.reads.union(&uses.reads);
                segment.writes.union(&uses.writes);
            }
            match &data.term {
                Terminator::Return => segment.returns = true,
                Terminator::Unreachable => {}
                Terminator::Step { .. } | Terminator::Rng { .. } | Terminator::SceneCall { .. } => {
                    segment.stops.push(block);
                }
                term @ (Terminator::Goto(_)
                | Terminator::SwitchBool { .. }
                | Terminator::SwitchVariant { .. }) => {
                    for next in term.successors() {
                        if seen.insert(next) {
                            queue.push_back(next);
                        }
                    }
                }
            }
        }
        segment.stops.sort_by_key(|block| index(*block));
        touched.union(&segment.reads);
        touched.union(&segment.writes);
        for stop in &segment.stops {
            if let Some(kept) = self.kept.get(stop) {
                touched.union(kept);
            }
        }
        (segment, touched.iter().collect())
    }

    fn regions(
        &self,
        entry: &Segment,
        suspensions: &[BasicBlock],
        segments: &FxHashMap<BasicBlock, Segment>,
        shape: &mut SceneShape,
    ) {
        // Its segment resumes a step or a draw, cannot return, runs known code only and
        // writes no story variable.
        let quiet = |block: BasicBlock| {
            let (Some(data), Some(segment)) = (self.body.block(block), segments.get(&block)) else {
                return false;
            };
            matches!(data.term, Terminator::Step { .. } | Terminator::Rng { .. })
                && !segment.returns
                && !segment.access.opaque
                && segment.access.written.is_empty()
        };
        // Neither its suspension nor its segment reads a story variable either.
        let pure = |block: BasicBlock| {
            quiet(block)
                && segments
                    .get(&block)
                    .is_some_and(|segment| segment.access.read.is_empty())
                && self
                    .info(block)
                    .is_some_and(|info| info.operands.read.is_empty())
        };
        let former = Former {
            segments,
            kept: self.kept,
        };
        for (header, blocks) in former.form(suspensions, &quiet, &entry.stops) {
            let outer = u32::try_from(shape.regions.len()).unwrap_or(u32::MAX);
            let region = self.region(None, header, &blocks, segments, true);
            let inner = if region.key_vars.is_empty() {
                Vec::new()
            } else {
                former.form(&blocks, &pure, &[header])
            };
            shape.headers.insert((None, header), outer);
            shape.regions.push(region);
            for (header, blocks) in inner {
                let index = u32::try_from(shape.regions.len()).unwrap_or(u32::MAX);
                shape.headers.insert((Some(outer), header), index);
                shape
                    .regions
                    .push(self.region(Some(outer), header, &blocks, segments, false));
            }
        }
    }

    /// The region of `header` with `blocks` (the header included); `vars`: whether it is
    /// keyed by the story variables it reads (a level-2 region reads none).
    fn region(
        &self,
        parent: Option<u32>,
        header: BasicBlock,
        blocks: &[BasicBlock],
        segments: &FxHashMap<BasicBlock, Segment>,
        vars: bool,
    ) -> Region {
        let mut reads = LocalSet::default();
        let mut key_vars = SlotSet::new();
        if let Some(info) = self.info(header) {
            reads.union(&info.operands.reads);
            key_vars.union(&info.operands.read);
        }
        for block in blocks {
            if let Some(segment) = segments.get(block) {
                reads.union(&segment.reads);
                key_vars.union(&segment.access.read);
            }
        }
        Region {
            parent,
            header,
            blocks: blocks.iter().copied().collect(),
            key_locals: self
                .kept
                .get(&header)
                .map(|kept| kept.iter().map(|local| reads.contains(local)).collect())
                .unwrap_or_default(),
            key_vars: if vars { key_vars } else { SlotSet::new() },
        }
    }
}

/// From which headers a block is reached within one round of [`Former::form`].
#[derive(Clone, Copy, PartialEq, Eq)]
enum Reach {
    None,
    One(BasicBlock),
    Many,
}

impl Reach {
    fn join(self, other: Reach) -> Reach {
        match (self, other) {
            (Reach::None, reach) | (reach, Reach::None) => reach,
            (Reach::One(a), Reach::One(b)) if a == b => Reach::One(a),
            _ => Reach::Many,
        }
    }
}

struct Former<'a> {
    /// The segment starting after each suspension block.
    segments: &'a FxHashMap<BasicBlock, Segment>,
    kept: &'a FxHashMap<BasicBlock, LocalSet>,
}

impl Former<'_> {
    /// Whether `header` can head a region: its own segment leaves its kept locals alone.
    fn eligible(&self, header: BasicBlock) -> bool {
        match (self.segments.get(&header), self.kept.get(&header)) {
            (Some(segment), Some(kept)) => !segment.writes.intersects(kept),
            _ => false,
        }
    }

    /// Whether `block` can belong to the region of `header`: it keeps what the header keeps,
    /// and its segment leaves that alone.
    fn admit(&self, header: BasicBlock, block: BasicBlock) -> bool {
        match (
            self.kept.get(&header),
            self.kept.get(&block),
            self.segments.get(&block),
        ) {
            (Some(kept), Some(inner), Some(segment)) => {
                kept.is_subset(inner) && !segment.writes.intersects(kept)
            }
            _ => false,
        }
    }

    fn stops(&self, block: BasicBlock) -> &[BasicBlock] {
        self.segments
            .get(&block)
            .map_or(&[], |segment| &segment.stops[..])
    }

    /// Partitions `candidates` (ascending) into regions: the headers that own a block, each
    /// with its blocks (itself and the blocks it owns, ascending), by ascending header. A
    /// block is owned by the header `h` when every segment that stops at it starts at `h` or
    /// at a block `h` owns, and `admit(h, block)`. A block that is not a `member`, or is
    /// reached from outside (`seeds`: the stops of the segments that start outside the
    /// candidates), from two headers or without `admit`, starts over: as a header when it is
    /// `eligible`, else as a block of no region.
    fn form(
        &self,
        candidates: &[BasicBlock],
        member: &dyn Fn(BasicBlock) -> bool,
        seeds: &[BasicBlock],
    ) -> Vec<(BasicBlock, Vec<BasicBlock>)> {
        let all: FxHashSet<BasicBlock> = candidates.iter().copied().collect();
        let mut headers: FxHashSet<BasicBlock> = FxHashSet::default();
        let mut plain: FxHashSet<BasicBlock> = candidates
            .iter()
            .copied()
            .filter(|block| !member(*block))
            .collect();
        loop {
            // Only a candidate that is neither a header nor plain gets a label.
            let open = |block: BasicBlock| {
                all.contains(&block) && !headers.contains(&block) && !plain.contains(&block)
            };
            let mut reach: FxHashMap<BasicBlock, Reach> = FxHashMap::default();
            let label = |reach: &FxHashMap<BasicBlock, Reach>, block: BasicBlock| {
                reach.get(&block).copied().unwrap_or(Reach::None)
            };
            for &block in seeds {
                if open(block) {
                    reach.insert(block, Reach::Many);
                }
            }
            for &block in candidates {
                let from = if plain.contains(&block) {
                    Reach::Many
                } else if headers.contains(&block) {
                    Reach::One(block)
                } else {
                    continue;
                };
                for &stop in self.stops(block) {
                    if open(stop) {
                        reach.insert(stop, label(&reach, stop).join(from));
                    }
                }
            }
            // A block with one header that admits it passes the header on.
            let mut queue: VecDeque<BasicBlock> = candidates
                .iter()
                .copied()
                .filter(|block| matches!(label(&reach, *block), Reach::One(_)))
                .collect();
            while let Some(block) = queue.pop_front() {
                let Reach::One(header) = label(&reach, block) else {
                    continue;
                };
                if !self.admit(header, block) {
                    continue;
                }
                for &stop in self.stops(block) {
                    if !open(stop) {
                        continue;
                    }
                    let old = label(&reach, stop);
                    let new = old.join(Reach::One(header));
                    if new != old {
                        reach.insert(stop, new);
                        queue.push_back(stop);
                    }
                }
            }
            let mut starts = Vec::new();
            for &block in candidates {
                if !open(block) {
                    continue;
                }
                let keep = match label(&reach, block) {
                    Reach::None => true,
                    Reach::One(header) => self.admit(header, block),
                    Reach::Many => false,
                };
                if !keep {
                    starts.push(block);
                }
            }
            if starts.is_empty() {
                let mut owned: FxHashMap<BasicBlock, Vec<BasicBlock>> = FxHashMap::default();
                for &block in candidates {
                    if let Reach::One(header) = label(&reach, block) {
                        owned.entry(header).or_default().push(block);
                    }
                }
                return candidates
                    .iter()
                    .filter_map(|header| {
                        let mut blocks = vec![*header];
                        blocks.append(owned.get_mut(header)?);
                        blocks.sort_by_key(|block| index(*block));
                        Some((*header, blocks))
                    })
                    .collect();
            }
            for block in starts {
                if self.eligible(block) {
                    headers.insert(block);
                } else {
                    plain.insert(block);
                }
            }
        }
    }
}
