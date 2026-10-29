//! Focus, input and controls: the focus order, keyboard and mouse handling of the top mount,
//! activation, sliders, selects and scrolling (`docs/site/src/content/docs/en/std/ui.md` §Focus).

use kagari_vm::{Trap, Value, Vm, value_eq};

use super::UiState;
use super::element::{Arena, ElemId, Kind};
use super::layout::{self, thumb_size};
use super::reactive::run_untracked;
use super::style::{ButtonStyleSpec, Interaction};
use crate::HostCtx;
use crate::frame::Rect;
use crate::input::{InputEvent, Key, MouseButton, PadButton};
use crate::values::SoundSpec;

/// Whether the UI used an input; ignored inputs go on to the bindings.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum UiInput {
    Consumed,
    Ignored,
}

/// A pressable input, to match a release with its press.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum InputKey {
    Key(Key),
    Mouse(MouseButton),
    Pad(PadButton),
}

/// The focus and pointer state of one mount.
#[derive(Clone, Default, Debug)]
pub(crate) struct FocusState {
    pub focused: Option<ElemId>,
    /// The position of `focused` in `order`.
    pub index: usize,
    pub hovered: Option<ElemId>,
    pub pressed: Option<ElemId>,
    pub dragging: Option<ElemId>,
    pub initialized: bool,
    /// The enabled controls in focus order, after the last layout.
    pub order: Vec<ElemId>,
    /// Bring the focused control into view after the next layout; `true` when focus moved down.
    pub scroll_request: Option<bool>,
}

/// What look selection reads of a mount's focus state.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct LookCtx {
    /// The focused control while focus is shown.
    focused: Option<ElemId>,
    hovered: Option<ElemId>,
    pressed: Option<ElemId>,
}

impl LookCtx {
    pub fn of(focus: &FocusState, focus_visible: bool) -> LookCtx {
        LookCtx {
            focused: focus.focused.filter(|_| focus_visible),
            hovered: focus.hovered,
            pressed: focus.pressed,
        }
    }

    pub fn interaction(&self, id: ElemId, kind: &Kind) -> Interaction {
        Interaction {
            disabled: !kind.is_enabled_control(),
            hovered: self.hovered == Some(id),
            focused: self.focused == Some(id),
            pressed: self.pressed == Some(id),
            on: matches!(kind, Kind::Toggle { value: true, .. }),
        }
    }
}

/// The index of the option whose value equals `value`.
pub(crate) fn current_option(
    options: &[Option<(Value, ElemId)>],
    value: Option<&Value>,
) -> Option<usize> {
    let value = value?;
    options.iter().position(|o| {
        o.as_ref()
            .is_some_and(|(v, _)| matches!(value_eq(v, value), Ok(true)))
    })
}

/// The children of `id` in draw order (the current label of a select is not a focus child).
pub(crate) fn draw_children(arena: &Arena, id: ElemId) -> Vec<ElemId> {
    match arena.get(id).map(|n| &n.kind) {
        Some(Kind::Select { .. }) | None => Vec::new(),
        Some(_) => arena.children(id),
    }
}

/// The enabled controls under `root`: those with `focus_order` first, ascending, then the rest,
/// each in document order.
pub(crate) fn order(arena: &Arena, root: ElemId) -> Vec<ElemId> {
    fn visit(arena: &Arena, id: ElemId, out: &mut Vec<(Option<u32>, ElemId)>) {
        let Some(node) = arena.get(id) else {
            return;
        };
        if node.kind.is_enabled_control() {
            out.push((node.mods.focus_order, id));
        }
        for child in draw_children(arena, id) {
            visit(arena, child, out);
        }
    }
    let mut all = Vec::new();
    visit(arena, root, &mut all);
    all.sort_by_key(|(order, _)| match order {
        Some(n) => (0u8, *n),
        None => (1, 0),
    });
    all.into_iter().map(|(_, id)| id).collect()
}

/// Takes the new focus order: first focus on the first layout of an autofocus mount; a focused
/// control that is gone or disabled hands focus to the entry at its index.
pub(crate) fn after_layout(focus: &mut FocusState, order: Vec<ElemId>, autofocus: bool) {
    if !focus.initialized {
        focus.initialized = true;
        focus.focused = if autofocus {
            order.first().copied()
        } else {
            None
        };
        focus.index = 0;
    } else if let Some(focused) = focus.focused {
        match order.iter().position(|id| *id == focused) {
            Some(index) => focus.index = index,
            None => {
                let index = focus.index.min(order.len().saturating_sub(1));
                focus.focused = order.get(index).copied();
                focus.index = index;
            }
        }
    }
    focus.order = order;
}

fn contains(r: &Rect, y_top: f32, y_bottom: f32) -> bool {
    y_top >= r.y && y_bottom <= r.y + r.h
}

/// Scrolls every scroll ancestor of `focused` so it shows: its top at the scroll's top, or its
/// bottom at the scroll's bottom when focus moved down and it fits. Whether an offset changed.
pub(crate) fn scroll_into_view(arena: &mut Arena, focused: ElemId, down: bool) -> bool {
    let Some(mut target) = arena.get(focused).map(|n| n.rect) else {
        return false;
    };
    let mut changed = false;
    let mut current = arena.get(focused).and_then(|n| n.parent);
    while let Some((parent, _)) = current {
        let Some(node) = arena.get_mut(parent) else {
            break;
        };
        // The content scrolls inside the padding, as layout and the wheel clamp it.
        let view = super::draw::inner_rect(node.rect, node.mods.padding);
        if let Kind::Scroll { offset, content } = &mut node.kind {
            let (top, bottom) = (target.y, target.y + target.h);
            if !contains(&view, top, bottom) {
                let delta = if down && target.h <= view.h {
                    bottom - (view.y + view.h)
                } else {
                    top - view.y
                };
                let next = (*offset + delta).clamp(0.0, (*content - view.h).max(0.0));
                if next != *offset {
                    // Layout has not run yet: move the target with its content, so the outer
                    // scrolls see where it now is.
                    target.y -= next - *offset;
                    *offset = next;
                    changed = true;
                }
            }
        }
        current = node.parent;
    }
    changed
}

/// Control `id` and the clip rects of the scrolls around it, in draw order.
fn hit_candidates(arena: &Arena, root: ElemId) -> Vec<(ElemId, Rect, Vec<Rect>)> {
    fn visit(
        arena: &Arena,
        id: ElemId,
        clips: &mut Vec<Rect>,
        out: &mut Vec<(ElemId, Rect, Vec<Rect>)>,
    ) {
        let Some(node) = arena.get(id) else {
            return;
        };
        out.push((id, node.rect, clips.clone()));
        let scroll = matches!(node.kind, Kind::Scroll { .. });
        if scroll {
            clips.push(node.rect);
        }
        for child in draw_children(arena, id) {
            visit(arena, child, clips, out);
        }
        if scroll {
            clips.pop();
        }
    }
    let mut out = Vec::new();
    visit(arena, root, &mut Vec::new(), &mut out);
    out
}

fn inside(r: &Rect, p: [f32; 2]) -> bool {
    p[0] >= r.x && p[0] < r.x + r.w && p[1] >= r.y && p[1] < r.y + r.h
}

/// The topmost element under `p` that `accept` takes, inside every scroll around it.
fn hit(arena: &Arena, root: ElemId, p: [f32; 2], accept: impl Fn(&Kind) -> bool) -> Option<ElemId> {
    hit_candidates(arena, root)
        .into_iter()
        .rev()
        .find(|(id, rect, clips)| {
            inside(rect, p)
                && clips.iter().all(|c| inside(c, p))
                && arena.get(*id).is_some_and(|n| accept(&n.kind))
        })
        .map(|(id, ..)| id)
}

/// The control under the pointer.
fn hit_control(ui: &UiState, root: ElemId) -> Option<ElemId> {
    hit(&ui.arena, root, ui.pointer?, Kind::is_enabled_control)
}

/// What handling an input asks for once the UI is no longer borrowed.
enum Effect {
    Call { f: Value, args: Vec<Value> },
    Sound(SoundSpec),
}

/// A built-in key: focus movement, activation, or anything else.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Nav {
    Up,
    Down,
    Left,
    Right,
    Tab,
    Activate,
    Shift,
    Other,
}

fn nav_of_key(key: Key) -> Nav {
    match key {
        Key::Up => Nav::Up,
        Key::Down => Nav::Down,
        Key::Left => Nav::Left,
        Key::Right => Nav::Right,
        Key::Tab => Nav::Tab,
        Key::Enter => Nav::Activate,
        Key::Shift => Nav::Shift,
        _ => Nav::Other,
    }
}

fn nav_of_pad(button: PadButton) -> Nav {
    match button {
        PadButton::DpadUp => Nav::Up,
        PadButton::DpadDown => Nav::Down,
        PadButton::DpadLeft => Nav::Left,
        PadButton::DpadRight => Nav::Right,
        PadButton::South => Nav::Activate,
        _ => Nav::Other,
    }
}

/// The `ButtonStyle` of a button, toggle or select (the default when none is set).
pub(crate) fn button_style(kind: &Kind) -> Option<ButtonStyleSpec> {
    match kind {
        Kind::Button { style, .. } | Kind::Toggle { style, .. } | Kind::Select { style, .. } => {
            Some(
                style
                    .as_deref()
                    .cloned()
                    .unwrap_or_else(ButtonStyleSpec::default_style),
            )
        }
        _ => None,
    }
}

fn hover_sound(arena: &Arena, id: ElemId) -> Option<Effect> {
    button_style(&arena.get(id)?.kind)?
        .hover_sound
        .map(Effect::Sound)
}

/// Activating control `id`: Button `on_click()`, Toggle `on_change(!value)`, Select
/// `on_change(next value)`; a slider does nothing.
fn activate(arena: &Arena, id: ElemId, effects: &mut Vec<Effect>) {
    let Some(node) = arena.get(id) else {
        return;
    };
    if !node.kind.is_enabled_control() {
        return;
    }
    let call = match &node.kind {
        Kind::Button { on_click, .. } => on_click.clone().map(|f| (f, Vec::new())),
        Kind::Toggle {
            value, on_change, ..
        } => on_change.clone().map(|f| (f, vec![Value::Bool(!value)])),
        Kind::Select {
            options,
            value,
            on_change,
            ..
        } => match (on_change, step_option(options, value.as_ref(), true)) {
            (Some(f), Some(v)) => Some((f.clone(), vec![v])),
            _ => None,
        },
        _ => return,
    };
    if let Some(sound) = button_style(&node.kind).and_then(|s| s.click_sound) {
        effects.push(Effect::Sound(sound));
    }
    if let Some((f, args)) = call {
        effects.push(Effect::Call { f, args });
    }
}

/// The value of the option after (or before) the current one, wrapping; the first option when
/// none is current.
fn step_option(
    options: &[Option<(Value, ElemId)>],
    value: Option<&Value>,
    next: bool,
) -> Option<Value> {
    let live: Vec<usize> = (0..options.len())
        .filter(|i| options.get(*i).is_some_and(Option::is_some))
        .collect();
    let current = current_option(options, value).and_then(|c| live.iter().position(|i| *i == c));
    let pos = match current {
        None => 0,
        Some(p) if next => p.saturating_add(1).checked_rem(live.len()).unwrap_or(0),
        Some(0) => live.len().saturating_sub(1),
        Some(p) => p.saturating_sub(1),
    };
    let index = *live.get(pos)?;
    options.get(index)?.as_ref().map(|(v, _)| v.clone())
}

/// The slider value at pointer x.
fn slider_value_at(arena: &Arena, id: ElemId, x: f32) -> Option<(i128, i128, Option<Value>)> {
    let node = arena.get(id)?;
    let Kind::Slider {
        value,
        lo,
        hi,
        step,
        on_change,
        style,
        ..
    } = &node.kind
    else {
        return None;
    };
    // The track is painted inside the padding (`draw.rs`), so the value maps from there.
    let r = super::draw::inner_rect(node.rect, node.mods.padding);
    let tw = thumb_size(style.as_deref())[0];
    let track = r.w - tw;
    let f = if track > 0.0 {
        ((x - (r.x + tw / 2.0)) / track).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let span = hi.checked_sub(*lo)? as f64;
    let step = i128::from((*step).max(1));
    let steps = (f64::from(f) * span / step as f64).round() as i128;
    let v = lo.checked_add(steps.checked_mul(step)?)?.clamp(*lo, *hi);
    Some((*value, v, on_change.clone()))
}

fn slider_change(arena: &Arena, id: ElemId, x: f32, effects: &mut Vec<Effect>) {
    if let Some((old, new, Some(f))) = slider_value_at(arena, id, x)
        && old != new
    {
        effects.push(Effect::Call {
            f,
            args: vec![Value::Int(new)],
        });
    }
}

/// Routes an input to the top mount; only it receives input.
pub(crate) fn handle_input(
    ctx: &mut HostCtx,
    vm: &mut Vm,
    event: &InputEvent,
) -> Result<UiInput, Trap> {
    layout::run(ctx.state);
    let ui = &mut ctx.state.ui;
    let Some(top) = ui.mounts.len().checked_sub(1) else {
        return Ok(UiInput::Ignored);
    };
    let mut effects = Vec::new();
    let result = match *event {
        InputEvent::Focus(focused) => {
            if !focused {
                ui.consumed.clear();
                ui.shift_held = false;
                // Every mount: a mode pushed while a slider was held must not leave it dragging.
                for mount in &mut ui.mounts {
                    mount.focus.pressed = None;
                    mount.focus.dragging = None;
                }
                ui.layout_dirty = true;
            }
            UiInput::Ignored
        }
        InputEvent::Key { key, pressed } => press(
            ui,
            top,
            InputKey::Key(key),
            nav_of_key(key),
            pressed,
            &mut effects,
        ),
        InputEvent::Pad { button, pressed } => press(
            ui,
            top,
            InputKey::Pad(button),
            nav_of_pad(button),
            pressed,
            &mut effects,
        ),
        InputEvent::MouseMove { x, y } => {
            mouse_move(ui, top, [x, y], &mut effects);
            UiInput::Ignored
        }
        InputEvent::MouseButton { button, pressed } => {
            mouse_button(ui, top, button, pressed, &mut effects)
        }
        InputEvent::Wheel { delta_y } => wheel(ui, top, delta_y),
    };
    let root_scope = ctx.state.ui.mounts.get(top).map(|m| m.root_scope);
    for effect in effects {
        match effect {
            Effect::Sound(sound) => ctx.state.audio.play_ui_sound(&sound),
            Effect::Call { f, args } => {
                if let Some(scope) = root_scope {
                    run_untracked(ctx, vm, scope, &f, args)?;
                }
            }
        }
    }
    Ok(result)
}

fn press(
    ui: &mut UiState,
    top: usize,
    key: InputKey,
    nav: Nav,
    pressed: bool,
    effects: &mut Vec<Effect>,
) -> UiInput {
    if nav == Nav::Shift {
        ui.shift_held = pressed;
        return UiInput::Ignored;
    }
    if !pressed {
        return if ui.consumed.remove(&key) {
            UiInput::Consumed
        } else {
            UiInput::Ignored
        };
    }
    let Some(mount) = ui.mounts.get_mut(top) else {
        return UiInput::Ignored;
    };
    let focus = &mut mount.focus;
    match nav {
        Nav::Other | Nav::Shift => UiInput::Ignored,
        Nav::Activate => {
            let Some(focused) = focus.focused.filter(|_| ui.focus_visible) else {
                return UiInput::Ignored;
            };
            if !ui
                .arena
                .get(focused)
                .is_some_and(|n| n.kind.is_enabled_control())
            {
                return UiInput::Ignored;
            }
            activate(&ui.arena, focused, effects);
            ui.consumed.insert(key);
            UiInput::Consumed
        }
        Nav::Up | Nav::Down | Nav::Left | Nav::Right | Nav::Tab => {
            if focus.order.is_empty() {
                return UiInput::Ignored;
            }
            ui.consumed.insert(key);
            ui.layout_dirty = true;
            if !ui.focus_visible {
                ui.focus_visible = true;
                if focus.focused.is_none() {
                    focus.focused = focus.order.first().copied();
                    focus.index = 0;
                }
                return UiInput::Consumed;
            }
            let horizontal = matches!(nav, Nav::Left | Nav::Right);
            if horizontal && let Some(focused) = focus.focused {
                let forward = nav == Nav::Right;
                match ui.arena.get(focused).map(|n| &n.kind) {
                    Some(Kind::Slider {
                        value,
                        lo,
                        hi,
                        step,
                        on_change,
                        ..
                    }) => {
                        let step = i128::from(*step);
                        let next = if forward {
                            value.saturating_add(step)
                        } else {
                            value.saturating_sub(step)
                        }
                        .clamp(*lo, *hi);
                        if next != *value
                            && let Some(f) = on_change
                        {
                            effects.push(Effect::Call {
                                f: f.clone(),
                                args: vec![Value::Int(next)],
                            });
                        }
                        return UiInput::Consumed;
                    }
                    Some(Kind::Select {
                        options,
                        value,
                        on_change,
                        ..
                    }) => {
                        if let (Some(f), Some(v)) =
                            (on_change, step_option(options, value.as_ref(), forward))
                        {
                            effects.push(Effect::Call {
                                f: f.clone(),
                                args: vec![v],
                            });
                        }
                        return UiInput::Consumed;
                    }
                    _ => {}
                }
            }
            let back = matches!(nav, Nav::Up | Nav::Left) || (nav == Nav::Tab && ui.shift_held);
            let len = focus.order.len();
            let current = focus
                .focused
                .and_then(|f| focus.order.iter().position(|id| *id == f));
            let index = match current {
                None => 0,
                Some(i) if back => i.checked_sub(1).unwrap_or(len.saturating_sub(1)),
                Some(i) => i.saturating_add(1).checked_rem(len).unwrap_or(0),
            };
            focus.index = index;
            focus.focused = focus.order.get(index).copied();
            focus.scroll_request = Some(!back);
            if let Some(effect) = focus.focused.and_then(|f| hover_sound(&ui.arena, f)) {
                effects.push(effect);
            }
            UiInput::Consumed
        }
    }
}

fn mouse_move(ui: &mut UiState, top: usize, p: [f32; 2], effects: &mut Vec<Effect>) {
    ui.pointer = Some(p);
    if ui.focus_visible {
        ui.focus_visible = false;
        ui.layout_dirty = true;
    }
    let Some(root) = ui.mounts.get(top).map(|m| m.root) else {
        return;
    };
    let hit = hit_control(ui, root);
    let Some(mount) = ui.mounts.get_mut(top) else {
        return;
    };
    if mount.focus.hovered != hit {
        mount.focus.hovered = hit;
        ui.layout_dirty = true;
        if let Some(effect) = hit.and_then(|c| hover_sound(&ui.arena, c)) {
            effects.push(effect);
        }
    }
    if let Some(slider) = mount.focus.dragging {
        slider_change(&ui.arena, slider, p[0], effects);
    }
}

fn mouse_button(
    ui: &mut UiState,
    top: usize,
    button: MouseButton,
    pressed: bool,
    effects: &mut Vec<Effect>,
) -> UiInput {
    let key = InputKey::Mouse(button);
    let Some(root) = ui.mounts.get(top).map(|m| m.root) else {
        return UiInput::Ignored;
    };
    if button != MouseButton::Left {
        return if !pressed && ui.consumed.remove(&key) {
            UiInput::Consumed
        } else {
            UiInput::Ignored
        };
    }
    let hit = hit_control(ui, root);
    let pointer_x = ui.pointer.map_or(0.0, |p| p[0]);
    let Some(mount) = ui.mounts.get_mut(top) else {
        return UiInput::Ignored;
    };
    ui.layout_dirty = true;
    if pressed {
        let Some(control) = hit else {
            return UiInput::Ignored;
        };
        mount.focus.pressed = Some(control);
        mount.focus.focused = Some(control);
        if let Some(index) = mount.focus.order.iter().position(|id| *id == control) {
            mount.focus.index = index;
        }
        if ui
            .arena
            .get(control)
            .is_some_and(|n| matches!(n.kind, Kind::Slider { .. }))
        {
            mount.focus.dragging = Some(control);
            slider_change(&ui.arena, control, pointer_x, effects);
        }
        ui.consumed.insert(key);
        return UiInput::Consumed;
    }
    let consumed = ui.consumed.remove(&key);
    if let Some(pressed) = mount.focus.pressed.take()
        && hit == Some(pressed)
    {
        activate(&ui.arena, pressed, effects);
    }
    // A press or drag begun under a mode that was pushed or popped since ends here too.
    for mount in &mut ui.mounts {
        mount.focus.pressed = None;
        mount.focus.dragging = None;
    }
    if consumed {
        UiInput::Consumed
    } else {
        UiInput::Ignored
    }
}

/// One wheel notch scrolls the innermost scroll under the pointer by 40 px.
pub(crate) fn wheel(ui: &mut UiState, top: usize, delta_y: f32) -> UiInput {
    if delta_y == 0.0 {
        return UiInput::Ignored;
    }
    let (Some(root), Some(p)) = (ui.mounts.get(top).map(|m| m.root), ui.pointer) else {
        return UiInput::Ignored;
    };
    let Some(scroll) = hit(&ui.arena, root, p, |k| matches!(k, Kind::Scroll { .. })) else {
        return UiInput::Ignored;
    };
    let Some(node) = ui.arena.get_mut(scroll) else {
        return UiInput::Ignored;
    };
    let inner_h = (node.rect.h - node.mods.padding[0] - node.mods.padding[2]).max(0.0);
    if let Kind::Scroll { offset, content } = &mut node.kind {
        *offset = (*offset - delta_y * 40.0).clamp(0.0, (*content - inner_h).max(0.0));
    }
    ui.layout_dirty = true;
    UiInput::Consumed
}
