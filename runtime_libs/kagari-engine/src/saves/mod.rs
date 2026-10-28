//! Save slots: naming, the slot index, saving, autosaving and loading (`docs/syntax/src/system.md`
//! §A save is a witness).

pub(crate) mod replay;
pub(crate) mod trace;

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use kagari_archive::save::Save;
use kagari_vm::{Closure, HostObj, Text, Trap, Value, Vm};

use crate::HostCtx;
use crate::frame::{Frame, ThumbKey};
use crate::store::value::Names;
use crate::story::{Current, Entry};
use crate::system::NavOp;
use crate::ui::SourceKey;
use crate::ui::imports::deref;
use crate::values::{StdTy, StdTypes};
use replay::{Divergence, DivergenceObj, replay};
use trace::{ParseError, SaveFile, parse, parse_head, version};

/// The width of a save thumbnail, in pixels.
pub(crate) const THUMB_WIDTH: u32 = 320;
/// The height of a save thumbnail, in pixels.
pub(crate) const THUMB_HEIGHT: u32 = 180;

/// A local date and time: the fields of `std::system::saves::Timestamp`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timestamp {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

/// What the slot listing shows of a save.
#[derive(Clone, Debug)]
pub(crate) struct SlotInfo {
    pub time: Timestamp,
    pub who: Option<Text>,
    pub text: Option<Text>,
}

/// The filled slots.
#[derive(Default)]
pub(crate) struct SaveIndex {
    pub slots: HashMap<ThumbKey, SlotInfo>,
}

/// The store name of a slot.
pub(crate) fn slot_name(key: ThumbKey) -> String {
    match key {
        ThumbKey::Manual(k) => format!("save-manual-{k}.kisave"),
        ThumbKey::Quick => "save-quick.kisave".to_owned(),
        ThumbKey::Auto(k) => format!("save-auto-{k}.kisave"),
    }
}

fn in_range(ctx: &HostCtx, key: ThumbKey) -> bool {
    let Some(config) = &ctx.state.system.config else {
        return false;
    };
    match key {
        ThumbKey::Manual(k) => k < config.save_slots,
        ThumbKey::Quick => true,
        ThumbKey::Auto(k) => k < config.auto_slots,
    }
}

/// A `SaveSlot` value; `None` out of range.
pub(crate) fn decode_slot(ctx: &HostCtx, v: &Value) -> Result<Option<ThumbKey>, Trap> {
    let (variant, fields) = ctx.state.std.variant(v, StdTy::SaveSlot)?;
    let index = || match fields.first() {
        Some(Value::Int(k)) => u32::try_from(*k).ok(),
        _ => None,
    };
    let key = match variant {
        0 => index().map(ThumbKey::Manual),
        1 => Some(ThumbKey::Quick),
        _ => index().map(ThumbKey::Auto),
    };
    Ok(key.filter(|k| in_range(ctx, *k)))
}

/// Reads every in-range slot's listing and thumbnail (boot).
pub(crate) fn load_all(ctx: &mut HostCtx) {
    let Some(config) = &ctx.state.system.config else {
        return;
    };
    let keys: Vec<ThumbKey> = (0..config.save_slots)
        .map(ThumbKey::Manual)
        .chain(std::iter::once(ThumbKey::Quick))
        .chain((0..config.auto_slots).map(ThumbKey::Auto))
        .collect();
    for key in keys {
        let name = slot_name(key);
        let bytes = match ctx.platform.read_store(&name) {
            Ok(Some(b)) => b,
            Ok(None) => continue,
            Err(e) => {
                log::warn!("{name}: unreadable save ignored: {e}");
                continue;
            }
        };
        let head = Save::from_bytes(bytes)
            .map_err(|e| e.to_string())
            .and_then(|save| parse_head(&ctx.state.program, &save));
        match head {
            Ok(head) => {
                if let Some(png) = head.thumbnail {
                    match image::load_from_memory_with_format(&png, image::ImageFormat::Png) {
                        Ok(img) => {
                            let img = img.into_rgba8();
                            let (w, h) = img.dimensions();
                            ctx.state
                                .assets
                                .set_thumbnail(key, Some((w, h, Arc::from(img.into_raw()))));
                        }
                        Err(e) => log::warn!("{name}: thumbnail ignored: {e}"),
                    }
                }
                ctx.state.system.saves.slots.insert(
                    key,
                    SlotInfo {
                        time: head.time,
                        who: head.who,
                        text: head.text,
                    },
                );
            }
            Err(e) => log::warn!("{name}: unreadable save ignored: {e}"),
        }
    }
}

fn encode_png(rgba: &[u8]) -> Result<Vec<u8>, png::EncodingError> {
    let mut buf = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut buf, THUMB_WIDTH, THUMB_HEIGHT);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header()?;
        w.write_image_data(rgba)?;
        w.finish()?;
    }
    Ok(buf)
}

/// The function path of an `on_exit` callback a save can name.
fn on_exit_path(ctx: &HostCtx, v: &Value) -> Option<String> {
    let program = &ctx.state.program;
    match v {
        Value::Func(f) => Some(program.function_path(*f).to_owned()),
        Value::Closure(c) if c.captures.is_empty() => {
            Some(program.function_path(c.func).to_owned())
        }
        _ => None,
    }
}

/// Saves the running game into `key`.
pub(crate) fn save_slot(ctx: &mut HostCtx, vm: &mut Vm, key: ThumbKey) {
    if !in_range(ctx, key) {
        return;
    }
    let Some(game) = ctx.state.game.as_ref() else {
        return;
    };
    if matches!(game.current, Current::None) {
        return;
    }
    let (who, text) = game
        .backlog
        .iter()
        .rev()
        .find_map(|e| match &e.entry {
            Entry::Line { who, text, .. } => Some((who.clone(), text.clone())),
            Entry::Chosen(_) => None,
        })
        .unwrap_or((None, None));
    let trace = game.trace.clone();
    let on_exit = game.on_exit.as_ref().and_then(|v| on_exit_path(ctx, v));
    let resolution = ctx.state.system.resolution().unwrap_or((1, 1));
    let rgba = match ctx.state.system.modes.first().map(|m| m.ui) {
        Some(mode) => {
            let nodes = crate::ui::draw::mount_nodes(ctx.state, mode, ctx.now_ms);
            let frame = Frame {
                size: resolution,
                nodes,
                capture_still: false,
            };
            ctx.platform
                .render_thumbnail(&frame, &ctx.state.assets, THUMB_WIDTH, THUMB_HEIGHT)
        }
        None => None,
    };
    let expected = (THUMB_WIDTH as usize)
        .checked_mul(THUMB_HEIGHT as usize)
        .and_then(|n| n.checked_mul(4));
    let rgba = rgba.filter(|b| Some(b.len()) == expected);
    let thumbnail = rgba.as_ref().and_then(|b| match encode_png(b) {
        Ok(png) => Some(png),
        Err(e) => {
            log::warn!("thumbnail not saved: {e}");
            None
        }
    });
    let time = ctx.platform.local_time();
    let file = SaveFile {
        version: 1,
        script_hash: ctx.state.system.script_hash.clone(),
        trace,
        on_exit,
        time,
        who: who.clone(),
        text: text.clone(),
        thumbnail,
    };
    let name = slot_name(key);
    let program = ctx.state.program.clone();
    let names = Names {
        program: &program,
        types: vm.types(),
    };
    let written = trace::write(&names, &file)
        .map_err(|e| e.to_string())
        .and_then(|save| {
            ctx.platform
                .write_store(&name, save.as_bytes())
                .map_err(|e| e.to_string())
        });
    if let Err(e) = written {
        log::error!("{name}: cannot write: {e}");
        return;
    }
    ctx.state
        .system
        .saves
        .slots
        .insert(key, SlotInfo { time, who, text });
    ctx.state
        .assets
        .set_thumbnail(key, rgba.map(|b| (THUMB_WIDTH, THUMB_HEIGHT, Arc::from(b))));
    ctx.state.ui.invalidate(SourceKey::SaveInfo(key));
    log::info!(target: "kagari::saves", "saved {key:?}");
}

/// Saves into the next auto slot.
pub(crate) fn autosave(ctx: &mut HostCtx, vm: &mut Vm) {
    let Some(slots) = ctx.state.system.config.as_ref().map(|c| c.auto_slots) else {
        return;
    };
    if slots == 0 {
        return;
    }
    let next = ctx.state.system.stores.auto_next;
    save_slot(
        ctx,
        vm,
        ThumbKey::Auto(next.checked_rem(slots).unwrap_or(0)),
    );
    ctx.state.system.stores.auto_next = next.wrapping_add(1).checked_rem(slots).unwrap_or(0);
    ctx.state.system.stores.dirty_meta = true;
}

/// Loads `key`: replays its trace and pushes the restored game.
pub(crate) fn load_slot(
    ctx: &mut HostCtx,
    vm: &mut Vm,
    key: Option<ThumbKey>,
) -> Result<(), Divergence> {
    let result = load(ctx, vm, key);
    let label = key.map_or_else(|| "an out-of-range slot".to_owned(), |k| format!("{k:?}"));
    if let Err(d) = &result {
        log::warn!(target: "kagari::saves", "load of {label} diverged: {}", d.describe());
    }
    result
}

fn load(ctx: &mut HostCtx, vm: &mut Vm, key: Option<ThumbKey>) -> Result<(), Divergence> {
    let key = key.ok_or(Divergence::Empty)?;
    let name = slot_name(key);
    let bytes = match ctx.platform.read_store(&name) {
        Ok(Some(b)) => b,
        Ok(None) => return Err(Divergence::Empty),
        Err(e) => return Err(Divergence::Unreadable(e.to_string())),
    };
    let save = Save::from_bytes(bytes).map_err(|e| Divergence::Unreadable(e.to_string()))?;
    let v = version(&save).map_err(Divergence::Unreadable)?;
    if v != 1 {
        return Err(Divergence::Version(v));
    }
    let program = ctx.state.program.clone();
    let file = parse(&program, &save).map_err(|e| match e {
        ParseError::Unreadable(e) => Divergence::Unreadable(e),
        ParseError::Args(entry) => Divergence::Args(entry),
    })?;
    let on_exit = restored_on_exit(ctx, &file);
    let frame = replay(ctx, vm, &file.trace, on_exit)?;
    let n = file.trace.choices.len();
    ctx.state.pending_nav.push(NavOp::Restore(frame));
    log::info!(target: "kagari::saves", "loaded {key:?} ({n} choices replayed)");
    Ok(())
}

fn restored_on_exit(ctx: &HostCtx, file: &SaveFile) -> Option<Value> {
    let program = &ctx.state.program;
    let found = file.on_exit.as_deref().and_then(|path| {
        let f = program.find_function(path)?;
        let func = program.module().functions.get(f.0 as usize)?;
        if func.params != 1 {
            return None;
        }
        if func.flags.closure {
            (file.script_hash == ctx.state.system.script_hash).then(|| {
                Value::Closure(Rc::new(Closure {
                    func: f,
                    captures: Vec::new(),
                }))
            })
        } else {
            Some(Value::Func(f))
        }
    });
    found.or_else(|| {
        let game = ctx.state.game.as_ref()?;
        (program.function_path(game.entry) == file.trace.entry)
            .then(|| game.on_exit.clone())
            .flatten()
    })
}

fn timestamp_value(std: &StdTypes, t: Timestamp) -> Value {
    std.make(
        StdTy::Timestamp,
        0,
        [
            i128::from(t.year),
            i128::from(t.month),
            i128::from(t.day),
            i128::from(t.hour),
            i128::from(t.minute),
            i128::from(t.second),
        ]
        .into_iter()
        .map(Value::Int)
        .collect(),
    )
}

import_table!(
    /// The `std::system::saves` imports.
    SavesImport {
        Info = "std::system::saves::info",
        Delete = "std::system::saves::delete",
        Describe = "std::system::saves::Divergence::describe",
    }
);

impl SavesImport {
    pub fn call(self, ctx: &mut HostCtx, vm: &mut Vm, args: Vec<Value>) -> Result<Value, Trap> {
        let path = self.path();
        let arg = |i: usize| crate::values::arg(vm, path, &args, i);
        match self {
            SavesImport::Info => {
                let Some(key) = decode_slot(ctx, &arg(0)?)? else {
                    return Ok(ctx.state.std.none());
                };
                ctx.state.ui.track(SourceKey::SaveInfo(key));
                let std = &ctx.state.std;
                let opt = |t: &Option<Text>| match t {
                    Some(t) => std.some(Value::Text(t.clone())),
                    None => std.none(),
                };
                Ok(match ctx.state.system.saves.slots.get(&key) {
                    Some(info) => std.some(std.make(
                        StdTy::SaveInfo,
                        0,
                        vec![
                            timestamp_value(std, info.time),
                            opt(&info.who),
                            opt(&info.text),
                        ],
                    )),
                    None => std.none(),
                })
            }
            SavesImport::Delete => {
                let Some(key) = decode_slot(ctx, &arg(0)?)? else {
                    return Ok(Value::unit());
                };
                let name = slot_name(key);
                if let Err(e) = ctx.platform.delete_store(&name) {
                    log::error!("{name}: cannot delete: {e}");
                }
                ctx.state.system.saves.slots.remove(&key);
                ctx.state.assets.set_thumbnail(key, None);
                ctx.state.ui.invalidate(SourceKey::SaveInfo(key));
                Ok(Value::unit())
            }
            SavesImport::Describe => {
                let v = deref(vm, args.first().cloned().unwrap_or(Value::Uninit))?;
                let d = match &v {
                    Value::Host(h) => h.downcast::<DivergenceObj>().cloned(),
                    _ => None,
                }
                .ok_or_else(|| Trap::host(format!("`{path}` expected a `Divergence`")))?;
                Ok(Value::Text(Text::Plain(Rc::from(d.0.describe()))))
            }
        }
    }
}

/// `nav::load`'s result value.
pub(crate) fn load_result(ctx: &HostCtx, r: Result<(), Divergence>) -> Value {
    let std = &ctx.state.std;
    match r {
        Ok(()) => std.ok(Value::unit()),
        Err(d) => std.err(Value::Host(HostObj::new(DivergenceObj(d)))),
    }
}
