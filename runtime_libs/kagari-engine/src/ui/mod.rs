//! `std::ui`: reactive element trees per mounted mode, layout, focus and input, and drawing.

pub(crate) mod draw;
pub(crate) mod element;
pub(crate) mod focus;
pub(crate) mod imports;
pub(crate) mod layout;
pub(crate) mod reactive;
pub(crate) mod style;
#[cfg(test)]
mod tests;
pub(crate) mod text;

use std::collections::HashSet;

use kagari_vm::{Trap, Value, Vm};

use crate::HostCtx;
use crate::system::modes::{ModeFrame, ModeKind};
use crate::values::StdTy;
use element::{Arena, ElemId, ElementObj};
use focus::{FocusState, InputKey};
use reactive::{CompId, CompKind, Graph, run_untracked};

pub(crate) use focus::{UiInput, handle_input};
pub(crate) use imports::UiImport;
pub(crate) use reactive::{SourceKey, call_host_fn, flush};

/// A mounted mode; allocated from a counter starting at 1, never reused.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct ModeId(u64);

/// One mounted mode: its root scope and element, and its focus state.
pub(crate) struct Mount {
    pub id: ModeId,
    pub kind: ModeKind,
    autofocus: bool,
    root_scope: CompId,
    root: ElemId,
    focus: FocusState,
}

/// The reactive graph, the element arena and the mounted modes, bottom to top.
pub(crate) struct UiState {
    graph: Graph,
    arena: Arena,
    mounts: Vec<Mount>,
    next_mode: u64,
    /// Set by anything that changes sizes or positions; `layout::run` clears it.
    pub layout_dirty: bool,
    /// The dialog whose still was captured (`draw::frame_nodes`).
    still_for: Option<ModeId>,
    /// Whether keyboard focus is shown: set by focus keys, cleared by mouse moves.
    focus_visible: bool,
    shift_held: bool,
    /// The presses the UI consumed, so their releases are consumed too.
    consumed: HashSet<InputKey>,
    /// The last mouse position in design px.
    pointer: Option<[f32; 2]>,
}

impl UiState {
    pub fn new() -> UiState {
        UiState {
            graph: Graph::default(),
            arena: Arena::default(),
            mounts: Vec::new(),
            next_mode: 1,
            layout_dirty: false,
            still_for: None,
            focus_visible: false,
            shift_held: false,
            consumed: HashSet::new(),
            pointer: None,
        }
    }

    /// Disposes a mounted mode and everything it owns.
    pub fn unmount(&mut self, id: ModeId) {
        let Some(index) = self.mounts.iter().position(|m| m.id == id) else {
            return;
        };
        let mount = self.mounts.remove(index);
        self.dispose(mount.root_scope);
        self.layout_dirty = true;
        log::debug!("ui: unmounted {:?} {id:?}", mount.kind);
    }

    /// Disposes every mounted mode, top first.
    pub fn unmount_all(&mut self) {
        while let Some(mount) = self.mounts.last() {
            let id = mount.id;
            self.unmount(id);
        }
    }

    fn push_mount(
        &mut self,
        id: ModeId,
        kind: ModeKind,
        autofocus: bool,
        root_scope: CompId,
        root: ElemId,
    ) {
        self.mounts.push(Mount {
            id,
            kind,
            autofocus,
            root_scope,
            root,
            focus: FocusState::default(),
        });
        self.layout_dirty = true;
    }
}

/// Mounts a mode: runs its component once, untracked, under a new root scope, and takes the
/// element it returned as the mode's root.
pub(crate) fn mount(ctx: &mut HostCtx, vm: &mut Vm, frame: &ModeFrame) -> Result<ModeId, Trap> {
    let ui = &mut ctx.state.ui;
    let id = ModeId(ui.next_mode);
    ui.next_mode = ui.next_mode.saturating_add(1);
    let root_scope = ui.graph.create(CompKind::Root, None, false);
    let args = frame.narrative.iter().cloned().collect();
    let root = run_untracked(ctx, vm, root_scope, &frame.component, args)
        .and_then(|value| root_element(ctx, vm, frame.kind, &value));
    let root = match root {
        Ok(root) => root,
        Err(trap) => {
            ctx.state.ui.dispose(root_scope);
            return Err(trap);
        }
    };
    let ui = &mut ctx.state.ui;
    let Some(node) = ui.arena.get_mut(root.id) else {
        ui.dispose(root_scope);
        return Err(Trap::host("element used after its mode was closed"));
    };
    if node.parent.is_some() {
        ui.dispose(root_scope);
        return Err(Trap::host("an element can be shown in one place only"));
    }
    node.mods = root.mods;
    ui.push_mount(id, frame.kind, frame.autofocus, root_scope, root.id);
    log::debug!("ui: mounted {:?} {id:?}", frame.kind);
    Ok(id)
}

/// The root element of a component's result: a plain `Element` for the game view, else the
/// mode newtype's field 0.
fn root_element(ctx: &HostCtx, vm: &Vm, kind: ModeKind, value: &Value) -> Result<ElementObj, Trap> {
    let value = imports::deref(vm, value.clone())?;
    let element = match kind {
        ModeKind::Game => value,
        ModeKind::Root | ModeKind::Overlay | ModeKind::GameOverlay | ModeKind::Dialog => {
            let ty = match kind {
                ModeKind::Root => StdTy::RootPage,
                ModeKind::Overlay => StdTy::Overlay,
                ModeKind::GameOverlay => StdTy::GameOverlay,
                _ => StdTy::Dialog,
            };
            let fields = ctx.state.std.fields(&value, ty)?;
            fields
                .first()
                .cloned()
                .ok_or_else(|| Trap::host("a mode component returned a malformed mode"))?
        }
    };
    imports::element_of(&element, "a mode component")
}
