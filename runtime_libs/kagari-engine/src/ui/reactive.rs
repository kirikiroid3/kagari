//! The reactive graph (SolidJS semantics): computations owned by scopes, the sources they read,
//! signals, invalidation and the flush that re-runs dirty computations.

use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use kagari_vm::{AssetId, FnKind, HostFn, StaticId, Trap, Value, Vm, value_eq};

use super::element::ElemId;
use super::{UiState, imports};
use crate::HostCtx;
use crate::frame::ThumbKey;

/// A reactive source: what a read import tracks and a writer invalidates.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum SourceKey {
    Signal(u64),
    Setting(StaticId),
    Meta(StaticId),
    NarrativeLine,
    NarrativeMenu,
    NarrativeHud,
    NarrativeHistory,
    NarrativeMode,
    SaveInfo(ThumbKey),
    SeenImage(AssetId),
    Heard(AssetId),
    Watched(AssetId),
}

/// A computation or scope of the graph.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct CompId {
    index: u32,
    generation: u32,
}

/// The node field a `Prop` computation sets.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PropSlot {
    TextStyle,
    TextContent,
    ImageFit,
    ImageSource,
    /// `game_screen!`'s `game`: read only for tracking.
    GameScreen,
    Disabled,
    ButtonStyle,
    SliderStyle,
    /// The value of a toggle, slider or select.
    Value,
    Step,
}

/// One `each` row: its item, its element and the scope that owns the element.
#[derive(Clone, Debug)]
pub(crate) struct Row {
    pub item: Value,
    pub elem: ElemId,
    pub scope: CompId,
}

#[derive(Clone, Debug)]
pub(crate) enum CompKind {
    /// A mount's scope.
    Root,
    /// An each row or a show branch: the owner of an untracked subtree.
    Row,
    Effect {
        run: Value,
    },
    /// A reactive attribute.
    Prop {
        elem: ElemId,
        slot: PropSlot,
        f: Value,
    },
    /// An element closure `|| e` (or `|| placed` in a layer).
    Child {
        parent: ElemId,
        index: u32,
        f: Value,
    },
    Each {
        elem: ElemId,
        items: Value,
        item: Value,
        rows: Vec<Row>,
    },
    Show {
        elem: ElemId,
        when: Value,
        then: Value,
        last: Option<bool>,
        branch: Option<CompId>,
    },
    SelectOption {
        elem: ElemId,
        index: u32,
        f: Value,
    },
}

impl CompKind {
    /// The closure a run calls; `None` for scopes, which never re-run.
    fn callee(&self) -> Option<&Value> {
        match self {
            CompKind::Root | CompKind::Row => None,
            CompKind::Effect { run } => Some(run),
            CompKind::Prop { f, .. }
            | CompKind::Child { f, .. }
            | CompKind::SelectOption { f, .. } => Some(f),
            CompKind::Each { items, .. } => Some(items),
            CompKind::Show { when, .. } => Some(when),
        }
    }
}

pub(crate) struct Comp {
    pub kind: CompKind,
    children: Vec<CompId>,
    elems: Vec<ElemId>,
    signals: Vec<u64>,
    sources: Vec<SourceKey>,
    seq: u64,
    dirty: bool,
}

/// A scope stack entry: who owns what is created, and who tracks what is read.
#[derive(Clone, Copy, Debug)]
struct Run {
    owner: Option<CompId>,
    tracking: Option<CompId>,
}

struct CompSlot {
    generation: u32,
    comp: Option<Comp>,
}

/// The reactive graph.
#[derive(Default)]
pub(crate) struct Graph {
    slots: Vec<CompSlot>,
    free: Vec<u32>,
    subs: HashMap<SourceKey, Vec<CompId>>,
    queue: BTreeMap<u64, CompId>,
    stack: Vec<Run>,
    next_seq: u64,
    signals: HashMap<u64, Value>,
    next_signal: u64,
}

impl Graph {
    pub fn get(&self, id: CompId) -> Option<&Comp> {
        let slot = self.slots.get(usize::try_from(id.index).ok()?)?;
        if slot.generation == id.generation {
            slot.comp.as_ref()
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, id: CompId) -> Option<&mut Comp> {
        let slot = self.slots.get_mut(usize::try_from(id.index).ok()?)?;
        if slot.generation == id.generation {
            slot.comp.as_mut()
        } else {
            None
        }
    }

    /// The owner of what is created now.
    pub fn owner(&self) -> Option<CompId> {
        self.stack.last().and_then(|run| run.owner)
    }

    /// A new computation owned by `owner`; `register` lists it among the owner's children, so
    /// the owner's re-run disposes it.
    pub fn create(&mut self, kind: CompKind, owner: Option<CompId>, register: bool) -> CompId {
        let seq = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(1);
        let comp = Comp {
            kind,
            children: Vec::new(),
            elems: Vec::new(),
            signals: Vec::new(),
            sources: Vec::new(),
            seq,
            dirty: false,
        };
        let id = if let Some(index) = self.free.pop()
            && let Some(slot) = usize::try_from(index)
                .ok()
                .and_then(|i| self.slots.get_mut(i))
        {
            slot.comp = Some(comp);
            CompId {
                index,
                generation: slot.generation,
            }
        } else {
            let index = u32::try_from(self.slots.len()).unwrap_or(u32::MAX);
            self.slots.push(CompSlot {
                generation: 0,
                comp: Some(comp),
            });
            CompId {
                index,
                generation: 0,
            }
        };
        if register && let Some(owner) = owner.and_then(|o| self.get_mut(o)) {
            owner.children.push(id);
        }
        id
    }

    /// Records an element created now in the current owner.
    pub fn own_elem(&mut self, elem: ElemId) {
        if let Some(owner) = self.owner().and_then(|o| self.get_mut(o)) {
            owner.elems.push(elem);
        }
    }

    /// A new signal owned by the current owner.
    fn signal(&mut self, init: Value) -> u64 {
        self.next_signal = self.next_signal.saturating_add(1);
        let n = self.next_signal;
        self.signals.insert(n, init);
        if let Some(owner) = self.owner().and_then(|o| self.get_mut(o)) {
            owner.signals.push(n);
        }
        n
    }

    fn unsubscribe(&mut self, id: CompId, sources: Vec<SourceKey>) {
        for key in sources {
            if let Some(list) = self.subs.get_mut(&key) {
                list.retain(|c| *c != id);
                if list.is_empty() {
                    self.subs.remove(&key);
                }
            }
        }
    }
}

impl UiState {
    /// Subscribes the tracking computation to `key`; a no-op outside a tracking scope.
    pub fn track(&mut self, key: SourceKey) {
        let Some(Run {
            tracking: Some(id), ..
        }) = self.graph.stack.last().copied()
        else {
            return;
        };
        let Some(comp) = self.graph.get_mut(id) else {
            return;
        };
        if comp.sources.contains(&key) {
            return;
        }
        comp.sources.push(key);
        self.graph.subs.entry(key).or_default().push(id);
    }

    /// Marks the dependents of `key` dirty; `flush` re-runs them.
    pub fn invalidate(&mut self, key: SourceKey) {
        let Some(subs) = self.graph.subs.get(&key).cloned() else {
            return;
        };
        for id in subs {
            if let Some(comp) = self.graph.get_mut(id)
                && !comp.dirty
            {
                comp.dirty = true;
                let seq = comp.seq;
                self.graph.queue.insert(seq, id);
            }
        }
    }

    /// Clears what the last run of `id` made: its child computations, elements and signals, and
    /// its subscriptions.
    fn reset(&mut self, id: CompId) {
        let Some(comp) = self.graph.get_mut(id) else {
            return;
        };
        comp.dirty = false;
        let seq = comp.seq;
        let children = std::mem::take(&mut comp.children);
        let elems = std::mem::take(&mut comp.elems);
        let signals = std::mem::take(&mut comp.signals);
        let sources = std::mem::take(&mut comp.sources);
        if self.graph.queue.get(&seq) == Some(&id) {
            self.graph.queue.remove(&seq);
        }
        for child in children {
            self.dispose(child);
        }
        for elem in elems {
            self.arena.free(elem);
        }
        for n in signals {
            self.graph.signals.remove(&n);
        }
        self.graph.unsubscribe(id, sources);
    }

    /// Disposes a computation and everything it owns.
    pub(crate) fn dispose(&mut self, id: CompId) {
        self.reset(id);
        let Some(comp) = self.graph.get_mut(id) else {
            return;
        };
        let kind = std::mem::replace(&mut comp.kind, CompKind::Row);
        match kind {
            CompKind::Each { rows, .. } => {
                for row in rows {
                    self.dispose(row.scope);
                }
            }
            CompKind::Show {
                branch: Some(branch),
                ..
            } => self.dispose(branch),
            _ => {}
        }
        if let Some(slot) = usize::try_from(id.index)
            .ok()
            .and_then(|i| self.graph.slots.get_mut(i))
            && slot.generation == id.generation
        {
            slot.comp = None;
            slot.generation = slot.generation.wrapping_add(1);
            self.graph.free.push(id.index);
        }
    }
}

/// Runs a computation: clears its last run, calls its closure tracked, then applies the result.
pub(crate) fn run_comp(ctx: &mut HostCtx, vm: &mut Vm, id: CompId) -> Result<(), Trap> {
    let ui = &mut ctx.state.ui;
    let Some(callee) = ui.graph.get(id).and_then(|c| c.kind.callee().cloned()) else {
        return Ok(());
    };
    ui.reset(id);
    ui.graph.stack.push(Run {
        owner: Some(id),
        tracking: Some(id),
    });
    let result = vm.call_value(ctx, &callee, FnKind::Fn, Vec::new());
    ctx.state.ui.graph.stack.pop();
    let value = result?;
    imports::apply(ctx, vm, id, value)
}

/// Calls `f` untracked, with what it creates owned by `owner`.
pub(crate) fn run_untracked(
    ctx: &mut HostCtx,
    vm: &mut Vm,
    owner: CompId,
    f: &Value,
    args: Vec<Value>,
) -> Result<Value, Trap> {
    ctx.state.ui.graph.stack.push(Run {
        owner: Some(owner),
        tracking: None,
    });
    let result = vm.call_value(ctx, f, FnKind::Fn, args);
    ctx.state.ui.graph.stack.pop();
    result
}

/// A new computation owned by the current owner, run now.
pub(crate) fn spawn(ctx: &mut HostCtx, vm: &mut Vm, kind: CompKind) -> Result<CompId, Trap> {
    let graph = &mut ctx.state.ui.graph;
    let owner = graph.owner();
    let id = graph.create(kind, owner, true);
    run_comp(ctx, vm, id)?;
    Ok(id)
}

/// `signal(init)`: a getter and a setter.
pub(crate) fn signal(ui: &mut UiState, init: Value) -> Value {
    let n = ui.graph.signal(init);
    let id = n.wrapping_shl(1);
    Value::Tuple(Rc::new(vec![
        Value::HostFn(HostFn { id, params: 0 }),
        Value::HostFn(HostFn {
            id: id | 1,
            params: 1,
        }),
    ]))
}

/// A signal getter (even id) or setter (odd id).
pub(crate) fn call_host_fn(
    ctx: &mut HostCtx,
    vm: &mut Vm,
    f: &HostFn,
    args: Vec<Value>,
) -> Result<Value, Trap> {
    let n = f.id.wrapping_shr(1);
    let ui = &mut ctx.state.ui;
    let Some(current) = ui.graph.signals.get(&n) else {
        return Err(Trap::host("signal used after its mode was closed"));
    };
    if f.id & 1 == 0 {
        let value = current.clone();
        ui.track(SourceKey::Signal(n));
        return Ok(value);
    }
    let value = crate::values::arg(vm, "signal setter", &args, 0)?;
    let ui = &mut ctx.state.ui;
    let unchanged = ui
        .graph
        .signals
        .get(&n)
        .is_some_and(|old| matches!(value_eq(old, &value), Ok(true)));
    if !unchanged {
        ui.graph.signals.insert(n, value);
        ui.invalidate(SourceKey::Signal(n));
    }
    Ok(Value::unit())
}

/// Re-runs dirty computations in creation order until none is left; 100 rounds at most.
pub(crate) fn flush(ctx: &mut HostCtx, vm: &mut Vm) -> Result<(), Trap> {
    for _ in 0..100 {
        let queue = &mut ctx.state.ui.graph.queue;
        if queue.is_empty() {
            return Ok(());
        }
        let batch: Vec<CompId> = std::mem::take(queue).into_values().collect();
        for id in batch {
            if ctx.state.ui.graph.get(id).is_some_and(|c| c.dirty) {
                run_comp(ctx, vm, id)?;
            }
        }
    }
    if ctx.state.ui.graph.queue.is_empty() {
        Ok(())
    } else {
        Err(Trap::host("reactive update did not settle"))
    }
}
