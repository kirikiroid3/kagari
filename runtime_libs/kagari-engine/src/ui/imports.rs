//! The `#[builtin]` functions of `std::ui`, and how computation results land on elements.

use std::rc::Rc;

use kagari_vm::{HostObj, RefRoot, Trap, Value, Vm, value_eq};

use super::UiState;
use super::element::{Align, ElemId, ElementObj, Fit, Kind, Mods, Place, PlacedObj};
use super::reactive::{self, CompId, CompKind, PropSlot, Row, run_untracked, spawn};
use super::style::{decode_button_style, decode_paint, decode_slider_style};
use crate::HostCtx;
use crate::anim::decimal_f32;
use crate::frame::ThumbKey;
use crate::screen::decode_text;
use crate::values::{
    StdTy, decode_anchor, decode_asset, decode_bool, decode_decimal, decode_int, decode_point,
};

import_table!(
    /// The `#[builtin]` functions of `std::ui`.
    UiImport {
        Signal = "std::ui::signal",
        Effect = "std::ui::effect",
        Each = "std::ui::each",
        Show = "std::ui::show",
        Label = "std::ui::label",
        Spacer = "std::ui::spacer",
        Scroll = "std::ui::scroll",
        Thumbnail = "std::ui::thumbnail",
        Message = "std::ui::message",
        Width = "std::ui::Element::width",
        Height = "std::ui::Element::height",
        Padding = "std::ui::Element::padding",
        Gap = "std::ui::Element::gap",
        Align = "std::ui::Element::align",
        Grow = "std::ui::Element::grow",
        Background = "std::ui::Element::background",
        Opacity = "std::ui::Element::opacity",
        FocusOrder = "std::ui::Element::focus_order",
        At = "std::ui::Element::at",
        Cover = "std::ui::Element::cover",
        Column = "std::ui::column",
        Row = "std::ui::row",
        Layer = "std::ui::layer",
        Text = "std::ui::text",
        Image = "std::ui::image",
        GameScreen = "std::ui::game_screen",
        Button = "std::ui::button",
        Slider = "std::ui::slider",
        Toggle = "std::ui::toggle",
        Select = "std::ui::select",
    }
);

/// `v`, loaded through a cell-rooted reference.
pub(crate) fn deref(vm: &Vm, v: Value) -> Result<Value, Trap> {
    match &v {
        Value::Ref(r) if matches!(r.root, RefRoot::Cell(_)) => vm.load(r),
        _ => Ok(v),
    }
}

fn not_a(path: &str, what: &str) -> Trap {
    Trap::host(format!("`{path}`: argument is not a {what}"))
}

/// The `Element` a value holds.
pub(crate) fn element_of(v: &Value, path: &str) -> Result<ElementObj, Trap> {
    match v {
        Value::Host(obj) => obj.downcast::<ElementObj>().cloned(),
        _ => None,
    }
    .ok_or_else(|| not_a(path, "std::ui::Element"))
}

fn placed_of(v: &Value, path: &str) -> Result<PlacedObj, Trap> {
    match v {
        Value::Host(obj) => obj.downcast::<PlacedObj>().cloned(),
        _ => None,
    }
    .ok_or_else(|| not_a(path, "std::ui::Placed"))
}

fn element_value(obj: ElementObj) -> Value {
    Value::Host(HostObj::new(obj))
}

/// A new element, owned by the current owner.
fn new_element(ui: &mut UiState, kind: Kind, slots: usize) -> ElemId {
    let id = ui.arena.alloc(kind, slots);
    ui.graph.own_elem(id);
    ui.layout_dirty = true;
    id
}

/// The closures of a `[Fn() -> Element; N]`.
fn closures(v: &Value, path: &str) -> Result<Vec<Value>, Trap> {
    match v {
        Value::Array(items) | Value::Vec(items) => Ok(items.to_vec()),
        _ => Err(not_a(path, "array of element closures")),
    }
}

/// The props struct `t` of an import.
fn props(ctx: &HostCtx, v: &Value, t: StdTy, path: &str, what: &str) -> Result<Vec<Value>, Trap> {
    ctx.state
        .std
        .fields(v, t)
        .map(<[Value]>::to_vec)
        .map_err(|_| not_a(path, what))
}

/// An `Option<Fn…>` prop.
fn optional(ctx: &HostCtx, v: &Value) -> Result<Option<Value>, Trap> {
    Ok(ctx.state.std.option(v)?.cloned())
}

/// Runs a reactive attribute now, and whenever what it reads changes.
fn prop(
    ctx: &mut HostCtx,
    vm: &mut Vm,
    elem: ElemId,
    slot: PropSlot,
    f: Value,
) -> Result<(), Trap> {
    spawn(ctx, vm, CompKind::Prop { elem, slot, f }).map(|_| ())
}

fn optional_prop(
    ctx: &mut HostCtx,
    vm: &mut Vm,
    elem: ElemId,
    slot: PropSlot,
    v: &Value,
) -> Result<(), Trap> {
    match optional(ctx, v)? {
        Some(f) => prop(ctx, vm, elem, slot, f),
        None => Ok(()),
    }
}

/// One `Child` computation per element closure, run in index order.
fn children(ctx: &mut HostCtx, vm: &mut Vm, parent: ElemId, fs: Vec<Value>) -> Result<(), Trap> {
    for (index, f) in fs.into_iter().enumerate() {
        let index = u32::try_from(index).map_err(|_| Trap::host("too many children"))?;
        spawn(ctx, vm, CompKind::Child { parent, index, f })?;
    }
    Ok(())
}

fn with_mods(
    vm: &Vm,
    path: &str,
    args: &[Value],
    f: impl FnOnce(&mut ElementObj, &Value) -> Result<(), Trap>,
) -> Result<Value, Trap> {
    let mut obj = element_of(&crate::values::arg(vm, path, args, 0)?, path)?;
    let arg = crate::values::arg(vm, path, args, 1)?;
    f(&mut obj, &arg)?;
    Ok(element_value(obj))
}

fn decode_align(ctx: &HostCtx, v: &Value) -> Result<Align, Trap> {
    let (variant, fields) = ctx.state.std.variant(v, StdTy::Align)?;
    Align::from_index(variant)
        .filter(|_| fields.is_empty())
        .ok_or_else(|| Trap::host("malformed `std::ui::Align`"))
}

fn decode_fit(ctx: &HostCtx, v: &Value) -> Result<Fit, Trap> {
    let (variant, fields) = ctx.state.std.variant(v, StdTy::Fit)?;
    Fit::from_index(variant)
        .filter(|_| fields.is_empty())
        .ok_or_else(|| Trap::host("malformed `std::ui::Fit`"))
}

/// A `SaveSlot` as the key of its thumbnail.
fn decode_slot(ctx: &HostCtx, v: &Value) -> Result<ThumbKey, Trap> {
    let (variant, fields) = ctx.state.std.variant(v, StdTy::SaveSlot)?;
    match (variant, fields) {
        (0, [k]) => Ok(ThumbKey::Manual(decode_int(k, "SaveSlot", "Manual")?)),
        (1, []) => Ok(ThumbKey::Quick),
        (2, [k]) => Ok(ThumbKey::Auto(decode_int(k, "SaveSlot", "Auto")?)),
        _ => Err(Trap::host("malformed `std::system::saves::SaveSlot`")),
    }
}

impl UiImport {
    /// Runs the import on `args`.
    pub fn call(self, ctx: &mut HostCtx, vm: &mut Vm, args: Vec<Value>) -> Result<Value, Trap> {
        use UiImport as I;
        let path = self.path();
        let arg = |i: usize| crate::values::arg(vm, path, &args, i);
        match self {
            I::Signal => {
                let init = arg(0)?;
                Ok(reactive::signal(&mut ctx.state.ui, init))
            }
            I::Effect => {
                let run = arg(0)?;
                spawn(ctx, vm, CompKind::Effect { run })?;
                Ok(Value::unit())
            }
            I::Each => {
                let (items, item) = (arg(0)?, arg(1)?);
                let elem = new_element(&mut ctx.state.ui, Kind::Each, 0);
                spawn(
                    ctx,
                    vm,
                    CompKind::Each {
                        elem,
                        items,
                        item,
                        rows: Vec::new(),
                    },
                )?;
                Ok(new_value(elem))
            }
            I::Show => {
                let (when, then) = (arg(0)?, arg(1)?);
                let elem = new_element(&mut ctx.state.ui, Kind::Show, 1);
                spawn(
                    ctx,
                    vm,
                    CompKind::Show {
                        elem,
                        when,
                        then,
                        last: None,
                        branch: None,
                    },
                )?;
                Ok(new_value(elem))
            }
            I::Label => {
                let text = decode_text(&arg(0)?)?;
                Ok(new_value(new_element(
                    &mut ctx.state.ui,
                    Kind::Label { text },
                    0,
                )))
            }
            I::Spacer => Ok(new_value(new_element(&mut ctx.state.ui, Kind::Spacer, 0))),
            I::Scroll => {
                let f = arg(0)?;
                let elem = new_element(
                    &mut ctx.state.ui,
                    Kind::Scroll {
                        offset: 0.0,
                        content: 0.0,
                    },
                    1,
                );
                children(ctx, vm, elem, vec![f])?;
                Ok(new_value(elem))
            }
            I::Thumbnail => {
                let slot = decode_slot(ctx, &arg(0)?)?;
                Ok(new_value(new_element(
                    &mut ctx.state.ui,
                    Kind::Thumbnail { slot },
                    0,
                )))
            }
            I::Message => {
                let style = arg(1)?;
                ctx.state.std.fields(&style, StdTy::TextStyle)?;
                Ok(new_value(new_element(
                    &mut ctx.state.ui,
                    Kind::Message { style },
                    0,
                )))
            }
            I::Width | I::Height | I::Gap | I::Grow | I::FocusOrder => {
                with_mods(vm, path, &args, |obj, v| {
                    let n: u32 = decode_int(v, "u32", "px")?;
                    let m = &mut obj.mods;
                    match self {
                        I::Width => m.width = Some(n),
                        I::Height => m.height = Some(n),
                        I::Gap => m.gap = n,
                        I::Grow => m.grow = Some(n),
                        _ => m.focus_order = Some(n),
                    }
                    Ok(())
                })
            }
            I::Padding => with_mods(vm, path, &args, |obj, v| {
                let f = ctx.state.std.fields(v, StdTy::Insets)?;
                for (slot, field) in obj.mods.padding.iter_mut().zip(f) {
                    *slot = (decode_int::<i32>(field, "Insets", "inset")? as f32).max(0.0);
                }
                Ok(())
            }),
            I::Opacity => with_mods(vm, path, &args, |obj, v| {
                let d = decode_decimal(v)?;
                if !d.is_fraction() {
                    return Err(Trap::host(
                        "`std::ui::Element::opacity`: opacity must be within 0.0..=1.0",
                    ));
                }
                obj.mods.opacity = decimal_f32(d);
                Ok(())
            }),
            I::Align => {
                let mut obj = element_of(&arg(0)?, path)?;
                obj.mods.align = Some((decode_align(ctx, &arg(1)?)?, decode_align(ctx, &arg(2)?)?));
                Ok(element_value(obj))
            }
            I::Background => {
                let mut obj = element_of(&arg(0)?, path)?;
                let paint = arg(1)?;
                obj.mods.background = decode_paint(ctx, &paint)?;
                Ok(element_value(obj))
            }
            I::At => {
                let elem = element_of(&arg(0)?, path)?;
                let [x, y] = decode_point(&ctx.state.std, &arg(1)?)?;
                let [ax, ay] = decode_anchor(&ctx.state.std, &arg(2)?)?;
                let place = Place::At {
                    x: x as f32,
                    y: y as f32,
                    ax: decimal_f32(ax),
                    ay: decimal_f32(ay),
                };
                Ok(Value::Host(HostObj::new(PlacedObj { elem, place })))
            }
            I::Cover => {
                let elem = element_of(&arg(0)?, path)?;
                Ok(Value::Host(HostObj::new(PlacedObj {
                    elem,
                    place: Place::Cover,
                })))
            }
            I::Column | I::Row | I::Layer => {
                let fs = closures(&arg(0)?, path)?;
                let kind = match self {
                    I::Column => Kind::Column,
                    I::Row => Kind::Row,
                    _ => Kind::Layer {
                        places: vec![None; fs.len()],
                    },
                };
                let elem = new_element(&mut ctx.state.ui, kind, fs.len());
                children(ctx, vm, elem, fs)?;
                Ok(new_value(elem))
            }
            I::Text => {
                let p = props(ctx, &arg(0)?, StdTy::TextProps, path, "std::ui::TextProps")?;
                let [style, content] = p.as_slice() else {
                    return Err(not_a(path, "std::ui::TextProps"));
                };
                let kind = Kind::Text {
                    style: None,
                    content: None,
                };
                let elem = new_element(&mut ctx.state.ui, kind, 0);
                optional_prop(ctx, vm, elem, PropSlot::TextStyle, style)?;
                prop(ctx, vm, elem, PropSlot::TextContent, content.clone())?;
                Ok(new_value(elem))
            }
            I::Image => {
                let p = props(
                    ctx,
                    &arg(0)?,
                    StdTy::ImageProps,
                    path,
                    "std::ui::ImageProps",
                )?;
                let [fit, source] = p.as_slice() else {
                    return Err(not_a(path, "std::ui::ImageProps"));
                };
                let kind = Kind::Image {
                    fit: Fit::Natural,
                    source: None,
                };
                let elem = new_element(&mut ctx.state.ui, kind, 0);
                optional_prop(ctx, vm, elem, PropSlot::ImageFit, fit)?;
                prop(ctx, vm, elem, PropSlot::ImageSource, source.clone())?;
                Ok(new_value(elem))
            }
            I::GameScreen => {
                let p = props(
                    ctx,
                    &arg(0)?,
                    StdTy::GameScreenProps,
                    path,
                    "std::ui::GameScreenProps",
                )?;
                let [game] = p.as_slice() else {
                    return Err(not_a(path, "std::ui::GameScreenProps"));
                };
                let elem = new_element(&mut ctx.state.ui, Kind::GameScreen, 0);
                prop(ctx, vm, elem, PropSlot::GameScreen, game.clone())?;
                Ok(new_value(elem))
            }
            I::Button => {
                let p = props(
                    ctx,
                    &arg(0)?,
                    StdTy::ButtonProps,
                    path,
                    "std::ui::ButtonProps",
                )?;
                let [on_click, disabled, style, kids] = p.as_slice() else {
                    return Err(not_a(path, "std::ui::ButtonProps"));
                };
                let fs = closures(kids, path)?;
                let kind = Kind::Button {
                    on_click: optional(ctx, on_click)?,
                    disabled: false,
                    style: None,
                };
                let elem = new_element(&mut ctx.state.ui, kind, fs.len());
                optional_prop(ctx, vm, elem, PropSlot::Disabled, disabled)?;
                optional_prop(ctx, vm, elem, PropSlot::ButtonStyle, style)?;
                children(ctx, vm, elem, fs)?;
                Ok(new_value(elem))
            }
            I::Slider => {
                let p = props(
                    ctx,
                    &arg(0)?,
                    StdTy::SliderProps,
                    path,
                    "std::ui::SliderProps",
                )?;
                let [value, on_change, step, disabled, style] = p.as_slice() else {
                    return Err(not_a(path, "std::ui::SliderProps"));
                };
                let lo: i128 = decode_int(&arg(1)?, "Int", "LO")?;
                let hi: i128 = decode_int(&arg(2)?, "Int", "HI")?;
                if lo > hi {
                    return Err(Trap::host("`std::ui::slider`: the range is empty"));
                }
                let kind = Kind::Slider {
                    value: lo,
                    lo,
                    hi,
                    step: 1,
                    on_change: optional(ctx, on_change)?,
                    disabled: false,
                    style: None,
                };
                let elem = new_element(&mut ctx.state.ui, kind, 0);
                prop(ctx, vm, elem, PropSlot::Value, value.clone())?;
                optional_prop(ctx, vm, elem, PropSlot::Step, step)?;
                optional_prop(ctx, vm, elem, PropSlot::Disabled, disabled)?;
                optional_prop(ctx, vm, elem, PropSlot::SliderStyle, style)?;
                Ok(new_value(elem))
            }
            I::Toggle => {
                let p = props(
                    ctx,
                    &arg(0)?,
                    StdTy::ToggleProps,
                    path,
                    "std::ui::ToggleProps",
                )?;
                let [value, on_change, disabled, style, kids] = p.as_slice() else {
                    return Err(not_a(path, "std::ui::ToggleProps"));
                };
                let fs = closures(kids, path)?;
                let kind = Kind::Toggle {
                    value: false,
                    on_change: optional(ctx, on_change)?,
                    disabled: false,
                    style: None,
                };
                let elem = new_element(&mut ctx.state.ui, kind, fs.len());
                prop(ctx, vm, elem, PropSlot::Value, value.clone())?;
                optional_prop(ctx, vm, elem, PropSlot::Disabled, disabled)?;
                optional_prop(ctx, vm, elem, PropSlot::ButtonStyle, style)?;
                children(ctx, vm, elem, fs)?;
                Ok(new_value(elem))
            }
            I::Select => {
                let p = props(
                    ctx,
                    &arg(0)?,
                    StdTy::SelectProps,
                    path,
                    "std::ui::SelectProps",
                )?;
                let [value, on_change, disabled, style, options] = p.as_slice() else {
                    return Err(not_a(path, "std::ui::SelectProps"));
                };
                let fs = closures(options, path)?;
                let kind = Kind::Select {
                    options: vec![None; fs.len()],
                    value: None,
                    on_change: optional(ctx, on_change)?,
                    disabled: false,
                    style: None,
                };
                let elem = new_element(&mut ctx.state.ui, kind, 0);
                prop(ctx, vm, elem, PropSlot::Value, value.clone())?;
                optional_prop(ctx, vm, elem, PropSlot::Disabled, disabled)?;
                optional_prop(ctx, vm, elem, PropSlot::ButtonStyle, style)?;
                for (index, f) in fs.into_iter().enumerate() {
                    let index = u32::try_from(index).map_err(|_| Trap::host("too many options"))?;
                    spawn(ctx, vm, CompKind::SelectOption { elem, index, f })?;
                }
                Ok(new_value(elem))
            }
        }
    }
}

fn new_value(id: ElemId) -> Value {
    element_value(ElementObj {
        id,
        mods: Default::default(),
    })
}

/// Applies the result of computation `id`'s run to the element it feeds.
pub(crate) fn apply(ctx: &mut HostCtx, vm: &mut Vm, id: CompId, value: Value) -> Result<(), Trap> {
    /// What the result feeds, without the rows an `each` keeps.
    enum Target {
        Nothing,
        Prop(ElemId, PropSlot),
        Child(ElemId, u32),
        Each(ElemId, Value),
        Show(ElemId, Value, Option<bool>),
        SelectOption(ElemId, u32),
    }
    let target = match ctx.state.ui.graph.get(id).map(|c| &c.kind) {
        None | Some(CompKind::Root | CompKind::Row | CompKind::Effect { .. }) => Target::Nothing,
        Some(CompKind::Prop { elem, slot, .. }) => Target::Prop(*elem, *slot),
        Some(CompKind::Child { parent, index, .. }) => Target::Child(*parent, *index),
        Some(CompKind::Each { elem, item, .. }) => Target::Each(*elem, item.clone()),
        Some(CompKind::Show {
            elem, then, last, ..
        }) => Target::Show(*elem, then.clone(), *last),
        Some(CompKind::SelectOption { elem, index, .. }) => Target::SelectOption(*elem, *index),
    };
    let value = deref(vm, value)?;
    match target {
        Target::Nothing => Ok(()),
        Target::Prop(elem, slot) => apply_prop(ctx, elem, slot, value),
        Target::Child(parent, index) => {
            let ui = &mut ctx.state.ui;
            let is_layer = matches!(
                ui.arena.get(parent).map(|n| &n.kind),
                Some(Kind::Layer { .. })
            );
            if is_layer {
                let placed = placed_of(&value, "a layer child")?;
                ui.arena.attach(parent, index, &placed.elem)?;
                if let Some(Kind::Layer { places }) = ui.arena.get_mut(parent).map(|n| &mut n.kind)
                    && let Some(slot) = usize::try_from(index).ok().and_then(|i| places.get_mut(i))
                {
                    *slot = Some(placed.place);
                }
            } else if ui.arena.contains(parent) {
                ui.arena
                    .attach(parent, index, &element_of(&value, "an element closure")?)?;
            }
            ui.layout_dirty = true;
            Ok(())
        }
        Target::Each(elem, item) => apply_each(ctx, vm, id, elem, &item, &value),
        Target::Show(elem, then, last) => apply_show(ctx, vm, id, elem, &then, last, &value),
        Target::SelectOption(elem, index) => {
            let fields = ctx.state.std.fields(&value, StdTy::SelectOption)?.to_vec();
            let [option, label] = fields.as_slice() else {
                return Err(Trap::host("malformed `std::ui::SelectOption`"));
            };
            let label = element_of(label, "std::ui::select")?;
            let ui = &mut ctx.state.ui;
            if ui.arena.contains(elem) {
                ui.arena
                    .attach_option(elem, index, option.clone(), &label)?;
            }
            ui.layout_dirty = true;
            Ok(())
        }
    }
}

fn apply_prop(ctx: &mut HostCtx, elem: ElemId, slot: PropSlot, value: Value) -> Result<(), Trap> {
    enum Set {
        TextStyle(super::text::TextStyleSpec),
        Content(kagari_vm::Text),
        Fit(Fit),
        Source(kagari_vm::AssetId),
        Disabled(bool),
        ButtonStyle(Rc<super::style::ButtonStyleSpec>),
        SliderStyle(Rc<super::style::SliderStyleSpec>),
        Value(Value),
        Step(u32),
    }
    let set = match slot {
        PropSlot::GameScreen => return Ok(()),
        PropSlot::TextStyle => {
            Set::TextStyle(ctx.state.text.decode_style(&ctx.state.assets, &value)?)
        }
        PropSlot::TextContent => Set::Content(decode_text(&value)?),
        PropSlot::ImageFit => Set::Fit(decode_fit(ctx, &value)?),
        PropSlot::ImageSource => {
            let id = decode_asset(&value)?;
            ctx.state.assets.ensure_image(id, ctx.platform)?;
            Set::Source(id)
        }
        PropSlot::Disabled => Set::Disabled(decode_bool(&value)?),
        PropSlot::ButtonStyle => Set::ButtonStyle(Rc::new(decode_button_style(ctx, &value)?)),
        PropSlot::SliderStyle => Set::SliderStyle(Rc::new(decode_slider_style(ctx, &value)?)),
        PropSlot::Value => Set::Value(value),
        PropSlot::Step => Set::Step(decode_int::<u32>(&value, "u32", "step")?.max(1)),
    };
    let ui = &mut ctx.state.ui;
    let Some(node) = ui.arena.get_mut(elem) else {
        return Ok(());
    };
    match (&mut node.kind, set) {
        (Kind::Text { style, .. }, Set::TextStyle(s)) => *style = Some(s),
        (Kind::Text { content, .. }, Set::Content(t)) => *content = Some(t),
        (Kind::Image { fit, .. }, Set::Fit(f)) => *fit = f,
        (Kind::Image { source, .. }, Set::Source(s)) => *source = Some(s),
        (
            Kind::Button { disabled, .. }
            | Kind::Toggle { disabled, .. }
            | Kind::Slider { disabled, .. }
            | Kind::Select { disabled, .. },
            Set::Disabled(d),
        ) => *disabled = d,
        (
            Kind::Button { style, .. } | Kind::Toggle { style, .. } | Kind::Select { style, .. },
            Set::ButtonStyle(s),
        ) => *style = Some(s),
        (Kind::Slider { style, .. }, Set::SliderStyle(s)) => *style = Some(s),
        (Kind::Toggle { value, .. }, Set::Value(v)) => *value = decode_bool(&v)?,
        (Kind::Slider { value, .. }, Set::Value(v)) => *value = decode_int(&v, "Int", "value")?,
        (Kind::Select { value, .. }, Set::Value(v)) => *value = Some(v),
        (Kind::Slider { step, .. }, Set::Step(s)) => *step = s,
        _ => return Err(Trap::host("a reactive attribute does not fit its element")),
    }
    ui.layout_dirty = true;
    Ok(())
}

/// `each`: reuses the first unused old row with an equal item, makes a row for each new item,
/// disposes the rest, and orders the children like the items.
fn apply_each(
    ctx: &mut HostCtx,
    vm: &mut Vm,
    id: CompId,
    elem: ElemId,
    item_fn: &Value,
    value: &Value,
) -> Result<(), Trap> {
    let items = match value {
        Value::Vec(items) | Value::Array(items) => items.to_vec(),
        _ => return Err(not_a("std::ui::each", "Vec")),
    };
    let mut old = match ctx.state.ui.graph.get_mut(id).map(|c| &mut c.kind) {
        Some(CompKind::Each { rows, .. }) => std::mem::take(rows),
        _ => return Ok(()),
    };
    let mut rows: Vec<Row> = Vec::with_capacity(items.len());
    // The modifiers each new row's element came with; reused rows keep theirs.
    let mut made_mods: Vec<Option<Mods>> = Vec::with_capacity(items.len());
    let mut result = Ok(());
    for item in items {
        if let Some(i) = old
            .iter()
            .position(|row| matches!(value_eq(&row.item, &item), Ok(true)))
        {
            rows.push(old.remove(i));
            made_mods.push(None);
            continue;
        }
        let scope = ctx.state.ui.graph.create(CompKind::Row, Some(id), false);
        let made = run_untracked(ctx, vm, scope, item_fn, vec![item.clone()])
            .and_then(|v| element_of(&deref(vm, v)?, "std::ui::each"));
        match made {
            Ok(obj) => {
                rows.push(Row {
                    item,
                    elem: obj.id,
                    scope,
                });
                made_mods.push(Some(obj.mods));
            }
            Err(trap) => {
                ctx.state.ui.dispose(scope);
                result = Err(trap);
                break;
            }
        }
    }
    let ui = &mut ctx.state.ui;
    for row in old {
        ui.dispose(row.scope);
    }
    // Re-slot every row: emptying first lets a reused row move to another index, while two
    // rows showing one element, or a stale element, still trap as `attach` does elsewhere.
    ui.arena.clear_children(elem);
    for (index, (row, mods)) in rows.iter().zip(made_mods).enumerate() {
        let index = u32::try_from(index).unwrap_or(u32::MAX);
        let mods = mods.or_else(|| ui.arena.get(row.elem).map(|n| n.mods.clone()));
        let attached = match mods {
            Some(mods) => ui
                .arena
                .attach(elem, index, &ElementObj { id: row.elem, mods }),
            None => Err(Trap::host("element used after its mode was closed")),
        };
        if let Err(trap) = attached {
            result = result.and(Err(trap));
        }
    }
    match ui.graph.get_mut(id).map(|c| &mut c.kind) {
        Some(CompKind::Each { rows: slot, .. }) => *slot = rows,
        _ => {
            for row in rows {
                ui.dispose(row.scope);
            }
        }
    }
    ui.layout_dirty = true;
    result
}

/// `show`: `then()` under a new row scope when `when()` turns true; disposed when it turns false.
fn apply_show(
    ctx: &mut HostCtx,
    vm: &mut Vm,
    id: CompId,
    elem: ElemId,
    then: &Value,
    last: Option<bool>,
    value: &Value,
) -> Result<(), Trap> {
    let when = decode_bool(value)?;
    if last == Some(when) {
        return Ok(());
    }
    let old_branch = match ctx.state.ui.graph.get_mut(id).map(|c| &mut c.kind) {
        Some(CompKind::Show { last, branch, .. }) => {
            *last = Some(when);
            branch.take()
        }
        _ => return Ok(()),
    };
    let ui = &mut ctx.state.ui;
    if let Some(branch) = old_branch {
        ui.dispose(branch);
    }
    ui.arena.detach(elem, 0);
    ui.layout_dirty = true;
    if !when {
        return Ok(());
    }
    let scope = ui.graph.create(CompKind::Row, Some(id), false);
    let made = run_untracked(ctx, vm, scope, then, Vec::new())
        .and_then(|v| element_of(&deref(vm, v)?, "std::ui::show"));
    let attached = made.and_then(|obj| ctx.state.ui.arena.attach(elem, 0, &obj));
    if let Err(trap) = attached {
        ctx.state.ui.dispose(scope);
        return Err(trap);
    }
    match ctx.state.ui.graph.get_mut(id).map(|c| &mut c.kind) {
        Some(CompKind::Show { branch, .. }) => *branch = Some(scope),
        _ => ctx.state.ui.dispose(scope),
    }
    Ok(())
}
