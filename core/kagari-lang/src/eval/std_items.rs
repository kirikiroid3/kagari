//! The std items const evaluation builds and inspects, found by their std path, and the
//! identification of every `#[builtin]` function.

use rustc_hash::FxHashMap;

use crate::db::Db;
use crate::def_map::{CrateId, TypeKey, def_maps};
use crate::input::{FilePath, Program, SourceFile};
use crate::item_tree::ItemIndex;
use crate::item_tree::{FnKind, ItemId, ItemKind, ItemParent, Shape, item_tree};

use kagari_shader::ShaderRole;

use super::value::{AssetKind, ConstValue, adt_variants};

/// A `#[builtin]` function (or `Integer` trait method) the evaluator implements.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Builtin {
    Asset,
    SystemFont,
    ShaderErase,
    ShaderArgsErase,
    VecNew,
    VecLen,
    VecIsEmpty,
    VecGet,
    VecSet,
    VecPush,
    IntegerInto,
    IntegerSaturate,
    RatNew,
    RatToDecimal,
    ScreenEmpty,
    ScreenEntity,
    ScreenContains,
    ScreenEntities,
    ScreenDespawn,
    ScreenHud,
    ScreenChoiceMenu,
    ScreenCamera,
    ScreenFilter,
    ScreenMusic,
    ScreenAmbience,
    ScreenPlay,
    ScreenStopSounds,
    ScreenReplayBgm,
    ScreenWait,
    ScreenBarrier,
    ScreenAtMark,
    ScreenAtEntry,
    ScreenBeginText,
    EntityPosition,
    EntityScale,
    EntityRotation,
    EntityOpacity,
    EntityFilter,
    EntityMix,
    EntityMask,
    EntityLocalMotion,
    EntityTravel,
    EntityFill,
    EntityPart,
    EntityLabel,
    EntityMovie,
    EntitySetPlane,
    EntitySetZ,
    EntitySetAnchor,
    EntitySetFlip,
    EntitySetFillSize,
    EntityGate,
    EntityApplyAnimation,
    PropGet,
    PropChange,
    ChangeApplyAnimation,
    ImageLayerGet,
    ImageLayerChange,
    ImageLayerPaint,
    ImageLayerClear,
    LabelLayerChange,
    LabelLayerClear,
    LabelLayerSetStyle,
    LabelLayerSetWidth,
    MovieLayerChange,
    MovieLayerClear,
    SwapApplyAnimation,
    HudTextWindow,
    HudQuickMenu,
    HudSideImage,
    ChoiceMenuDisabledArm,
    ChoiceMenuSetAlign,
    CameraOffset,
    CameraZoom,
    CameraRotation,
    CameraApplyAnimation,
    ChannelGet,
    ChannelChange,
    ChannelStop,
    ChannelVolume,
    ChannelChangeApplyAnimation,
    Ruby,
    MonoRuby,
    Effect,
    Concat,
    ToText,
    Setting,
    Meta,
    /// A `fn` or `pushdown fn` builtin: never evaluated at compile time.
    Runtime,
}

/// The number of const builtins the evaluator implements.
#[cfg(test)]
pub(crate) const BUILTINS_LEN: usize = BUILTINS.len();

/// `(std file, owner type or trait, function name)` of every const builtin.
const BUILTINS: &[(&str, &str, &str, Builtin)] = &[
    ("mod.kis", "", "asset", Builtin::Asset),
    ("mod.kis", "", "system_font", Builtin::SystemFont),
    (
        "screen/shader.kis",
        "",
        "erase_shader",
        Builtin::ShaderErase,
    ),
    (
        "screen/shader.kis",
        "",
        "erase_args",
        Builtin::ShaderArgsErase,
    ),
    ("mod.kis", "Vec", "new", Builtin::VecNew),
    ("mod.kis", "Vec", "len", Builtin::VecLen),
    ("mod.kis", "Vec", "is_empty", Builtin::VecIsEmpty),
    ("mod.kis", "Vec", "get", Builtin::VecGet),
    ("mod.kis", "Vec", "set", Builtin::VecSet),
    ("mod.kis", "Vec", "push", Builtin::VecPush),
    ("mod.kis", "Integer", "into", Builtin::IntegerInto),
    ("mod.kis", "Integer", "saturate", Builtin::IntegerSaturate),
    ("mod.kis", "Rat", "new", Builtin::RatNew),
    ("mod.kis", "Rat", "to_decimal", Builtin::RatToDecimal),
    ("screen.kis", "Screen", "empty", Builtin::ScreenEmpty),
    ("screen.kis", "Screen", "entity", Builtin::ScreenEntity),
    ("screen.kis", "Screen", "contains", Builtin::ScreenContains),
    ("screen.kis", "Screen", "entities", Builtin::ScreenEntities),
    ("screen.kis", "Screen", "despawn", Builtin::ScreenDespawn),
    ("screen.kis", "Screen", "hud", Builtin::ScreenHud),
    (
        "screen.kis",
        "Screen",
        "choice_menu",
        Builtin::ScreenChoiceMenu,
    ),
    ("screen.kis", "Screen", "camera", Builtin::ScreenCamera),
    ("screen.kis", "Screen", "filter", Builtin::ScreenFilter),
    ("screen.kis", "Screen", "music", Builtin::ScreenMusic),
    ("screen.kis", "Screen", "ambience", Builtin::ScreenAmbience),
    ("screen.kis", "Screen", "play", Builtin::ScreenPlay),
    (
        "screen.kis",
        "Screen",
        "stop_sounds",
        Builtin::ScreenStopSounds,
    ),
    (
        "screen.kis",
        "Screen",
        "replay_bgm",
        Builtin::ScreenReplayBgm,
    ),
    ("screen.kis", "Screen", "wait", Builtin::ScreenWait),
    ("screen.kis", "Screen", "barrier", Builtin::ScreenBarrier),
    ("screen.kis", "Screen", "at_mark", Builtin::ScreenAtMark),
    ("screen.kis", "Screen", "at_entry", Builtin::ScreenAtEntry),
    (
        "screen.kis",
        "Screen",
        "begin_text",
        Builtin::ScreenBeginText,
    ),
    ("screen.kis", "Entity", "position", Builtin::EntityPosition),
    ("screen.kis", "Entity", "scale", Builtin::EntityScale),
    ("screen.kis", "Entity", "rotation", Builtin::EntityRotation),
    ("screen.kis", "Entity", "opacity", Builtin::EntityOpacity),
    ("screen.kis", "Entity", "filter", Builtin::EntityFilter),
    ("screen.kis", "Entity", "mix", Builtin::EntityMix),
    ("screen.kis", "Entity", "mask", Builtin::EntityMask),
    (
        "screen.kis",
        "Entity",
        "local_motion",
        Builtin::EntityLocalMotion,
    ),
    ("screen.kis", "Entity", "travel", Builtin::EntityTravel),
    ("screen.kis", "Entity", "fill", Builtin::EntityFill),
    ("screen.kis", "Entity", "part", Builtin::EntityPart),
    ("screen.kis", "Entity", "label", Builtin::EntityLabel),
    ("screen.kis", "Entity", "movie", Builtin::EntityMovie),
    ("screen.kis", "Entity", "set_plane", Builtin::EntitySetPlane),
    ("screen.kis", "Entity", "set_z", Builtin::EntitySetZ),
    (
        "screen.kis",
        "Entity",
        "set_anchor",
        Builtin::EntitySetAnchor,
    ),
    ("screen.kis", "Entity", "set_flip", Builtin::EntitySetFlip),
    (
        "screen.kis",
        "Entity",
        "set_fill_size",
        Builtin::EntitySetFillSize,
    ),
    ("screen.kis", "Entity", "gate", Builtin::EntityGate),
    (
        "screen.kis",
        "Entity",
        "apply_animation",
        Builtin::EntityApplyAnimation,
    ),
    ("screen.kis", "Prop", "get", Builtin::PropGet),
    ("screen.kis", "Prop", "change", Builtin::PropChange),
    (
        "screen.kis",
        "Change",
        "apply_animation",
        Builtin::ChangeApplyAnimation,
    ),
    ("screen.kis", "ImageLayer", "get", Builtin::ImageLayerGet),
    (
        "screen.kis",
        "ImageLayer",
        "change",
        Builtin::ImageLayerChange,
    ),
    (
        "screen.kis",
        "ImageLayer",
        "paint",
        Builtin::ImageLayerPaint,
    ),
    (
        "screen.kis",
        "ImageLayer",
        "clear",
        Builtin::ImageLayerClear,
    ),
    (
        "screen.kis",
        "LabelLayer",
        "change",
        Builtin::LabelLayerChange,
    ),
    (
        "screen.kis",
        "LabelLayer",
        "clear",
        Builtin::LabelLayerClear,
    ),
    (
        "screen.kis",
        "LabelLayer",
        "set_style",
        Builtin::LabelLayerSetStyle,
    ),
    (
        "screen.kis",
        "LabelLayer",
        "set_width",
        Builtin::LabelLayerSetWidth,
    ),
    (
        "screen.kis",
        "MovieLayer",
        "change",
        Builtin::MovieLayerChange,
    ),
    (
        "screen.kis",
        "MovieLayer",
        "clear",
        Builtin::MovieLayerClear,
    ),
    (
        "screen.kis",
        "Swap",
        "apply_animation",
        Builtin::SwapApplyAnimation,
    ),
    ("screen.kis", "Hud", "text_window", Builtin::HudTextWindow),
    ("screen.kis", "Hud", "quick_menu", Builtin::HudQuickMenu),
    ("screen.kis", "Hud", "side_image", Builtin::HudSideImage),
    (
        "screen.kis",
        "ChoiceMenu",
        "disabled_arm",
        Builtin::ChoiceMenuDisabledArm,
    ),
    (
        "screen.kis",
        "ChoiceMenu",
        "set_align",
        Builtin::ChoiceMenuSetAlign,
    ),
    ("screen.kis", "Camera", "offset", Builtin::CameraOffset),
    ("screen.kis", "Camera", "zoom", Builtin::CameraZoom),
    ("screen.kis", "Camera", "rotation", Builtin::CameraRotation),
    (
        "screen.kis",
        "Camera",
        "apply_animation",
        Builtin::CameraApplyAnimation,
    ),
    ("screen/audio.kis", "Channel", "get", Builtin::ChannelGet),
    (
        "screen/audio.kis",
        "Channel",
        "change",
        Builtin::ChannelChange,
    ),
    ("screen/audio.kis", "Channel", "stop", Builtin::ChannelStop),
    (
        "screen/audio.kis",
        "Channel",
        "volume",
        Builtin::ChannelVolume,
    ),
    (
        "screen/audio.kis",
        "ChannelChange",
        "apply_animation",
        Builtin::ChannelChangeApplyAnimation,
    ),
    ("narrative/text.kis", "", "ruby", Builtin::Ruby),
    ("narrative/text.kis", "", "mono_ruby", Builtin::MonoRuby),
    ("narrative/text.kis", "", "effect", Builtin::Effect),
    ("narrative/text.kis", "", "concat", Builtin::Concat),
    ("narrative/text.kis", "", "to_text", Builtin::ToText),
    ("system.kis", "", "setting", Builtin::Setting),
    ("narrative.kis", "", "meta", Builtin::Meta),
];

/// An ADT and the indices of its fields (or variants) by name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdtInfo {
    pub item: ItemId,
    /// Variant names of an enum; for a struct, its single name.
    pub variants: Vec<String>,
    /// Field names of a record struct (empty for tuple and unit structs).
    pub fields: Vec<String>,
}

impl AdtInfo {
    pub fn field(&self, name: &str) -> Option<usize> {
        self.fields.iter().position(|field| field == name)
    }

    pub fn variant(&self, name: &str) -> Option<u32> {
        self.variants
            .iter()
            .position(|variant| variant == name)
            .and_then(|index| u32::try_from(index).ok())
    }

    /// A unit variant of this enum.
    pub fn unit(&self, variant: &str) -> ConstValue {
        ConstValue::Adt {
            item: self.item,
            variant: self.variant(variant).unwrap_or(0),
            fields: imbl::Vector::new(),
        }
    }

    /// The struct with `fields` in declaration order.
    pub fn make(&self, fields: Vec<ConstValue>) -> ConstValue {
        ConstValue::Adt {
            item: self.item,
            variant: 0,
            fields: fields.into(),
        }
    }
}

/// The std items the evaluator and the screen model use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StdTable {
    pub option: AdtInfo,
    pub result: AdtInfo,
    pub range: AdtInfo,
    pub text: ItemId,
    pub screen: ItemId,
    pub entity_id: AdtInfo,
    pub part: AdtInfo,
    pub point: AdtInfo,
    pub offset: AdtInfo,
    pub size: AdtInfo,
    pub anchor: AdtInfo,
    pub angle: AdtInfo,
    pub color: AdtInfo,
    pub plane: AdtInfo,
    pub visibility: AdtInfo,
    pub flip: AdtInfo,
    pub pose: AdtInfo,
    pub valign: AdtInfo,
    pub mark: AdtInfo,
    pub text_effect: AdtInfo,
    pub duration: AdtInfo,
    pub easing: AdtInfo,
    pub tween: AdtInfo,
    pub transition: AdtInfo,
    pub transitional_motion: AdtInfo,
    pub motion: AdtInfo,
    pub motion_key: AdtInfo,
    pub sound: AdtInfo,
    pub say: AdtInfo,
    pub choice: AdtInfo,
    pub choice_arm: AdtInfo,
    pub setting: ItemId,
    pub meta: ItemId,
    pub push_step: ItemId,
    pub rng_choose: ItemId,
    /// `std::ui::slider`, whose import also takes its `LO` and `HI`.
    pub ui_slider: ItemId,
    /// The std file `time.kis`, whose integer overflows are `Duration` overflows.
    pub time_file: SourceFile,
    pub assets: Vec<(ItemId, AssetKind)>,
    pub builtins: FxHashMap<ItemId, Builtin>,
}

/// The std items the evaluator needs; `None` when one is missing (std failed to load).
#[salsa::tracked(returns(ref))]
pub fn std_table(db: &dyn Db, program: Program) -> Option<StdTable> {
    let maps = def_maps(db, program);
    let std_files: FxHashMap<std::sync::Arc<str>, SourceFile> = maps
        .files
        .iter()
        .filter(|(_, krate)| *krate == CrateId::Std)
        .filter_map(|(file, _)| match file.path(db) {
            FilePath::Std(name) => Some((name.clone(), *file)),
            FilePath::Disk(_) => None,
        })
        .collect();
    let find = |file: &str, name: &str| -> Option<ItemId> {
        let source = *std_files.get(file)?;
        let tree = item_tree(db, source);
        tree.top.iter().find_map(|&index| {
            let item = tree.item(index)?;
            (item.name.as_deref() == Some(name)).then_some(ItemId {
                file: source,
                index,
            })
        })
    };
    let adt = |file: &str, name: &str| -> Option<AdtInfo> {
        let item = find(file, name)?;
        let variants = adt_variants(db, item);
        let fields = match variants.first() {
            Some((_, Shape::Record(names))) if variants.len() == 1 => {
                names.iter().map(|name| name.to_string()).collect()
            }
            _ => Vec::new(),
        };
        Some(AdtInfo {
            item,
            variants: variants.into_iter().map(|(name, _)| name).collect(),
            fields,
        })
    };
    let mut assets = Vec::new();
    for kind in AssetKind::FILES {
        assets.push((find("mod.kis", kind.type_name())?, kind));
    }
    for role in ShaderRole::ALL {
        assets.push((
            find("screen/shader.kis", role.type_name())?,
            AssetKind::Shader(role),
        ));
    }
    let mut builtins = FxHashMap::default();
    for (name, &file) in &std_files {
        let tree = item_tree(db, file);
        for (index, item) in tree.items.iter().enumerate() {
            let ItemKind::Fn { has_body, kind, .. } = item.kind else {
                continue;
            };
            if has_body {
                continue;
            }
            let Some(index) = ItemIndex::from_usize(index) else {
                continue;
            };
            let owner = match item.parent {
                ItemParent::Item(parent) => {
                    let parent_id = ItemId {
                        file,
                        index: parent,
                    };
                    match tree.item(parent).map(|data| &data.kind) {
                        Some(ItemKind::Impl { .. }) => match maps.impl_keys.get(&parent_id) {
                            Some(TypeKey::Item(ty)) => item_tree(db, ty.file)
                                .item(ty.index)
                                .and_then(|data| data.name.clone())
                                .map(|name| name.to_string())
                                .unwrap_or_default(),
                            _ => String::new(),
                        },
                        _ => tree
                            .item(parent)
                            .and_then(|data| data.name.clone())
                            .map(|name| name.to_string())
                            .unwrap_or_default(),
                    }
                }
                _ => String::new(),
            };
            let fn_name = item.name.as_deref().unwrap_or_default();
            let builtin = BUILTINS
                .iter()
                .find(|(file, ty, func, _)| {
                    *file == &**name && *ty == owner.as_str() && *func == fn_name
                })
                .map(|entry| entry.3);
            let builtin = match (builtin, kind) {
                (Some(builtin), FnKind::Const) => builtin,
                _ => Builtin::Runtime,
            };
            builtins.insert(ItemId { file, index }, builtin);
        }
    }
    Some(StdTable {
        option: adt("mod.kis", "Option")?,
        result: adt("mod.kis", "Result")?,
        range: adt("mod.kis", "Range")?,
        text: find("narrative/text.kis", "Text")?,
        screen: find("screen.kis", "Screen")?,
        entity_id: adt("screen.kis", "EntityId")?,
        part: adt("screen.kis", "Part")?,
        point: adt("screen.kis", "Point")?,
        offset: adt("screen.kis", "Offset")?,
        size: adt("screen.kis", "Size")?,
        anchor: adt("screen.kis", "Anchor")?,
        angle: adt("screen.kis", "Angle")?,
        color: adt("screen.kis", "Color")?,
        plane: adt("screen.kis", "Plane")?,
        visibility: adt("screen.kis", "Visibility")?,
        flip: adt("screen.kis", "Flip")?,
        pose: adt("screen.kis", "Pose")?,
        valign: adt("screen.kis", "VAlign")?,
        mark: adt("narrative/text.kis", "Mark")?,
        text_effect: adt("narrative/text.kis", "TextEffect")?,
        duration: adt("time.kis", "Duration")?,
        easing: adt("screen/anim.kis", "Easing")?,
        tween: adt("screen/anim.kis", "Tween")?,
        transition: adt("screen/anim.kis", "Transition")?,
        transitional_motion: adt("screen/shader.kis", "TransitionalMotion")?,
        motion: adt("screen/anim.kis", "Motion")?,
        motion_key: adt("screen/anim.kis", "MotionKey")?,
        sound: adt("screen/audio.kis", "Sound")?,
        say: adt("narrative.kis", "Say")?,
        choice: adt("narrative.kis", "Choice")?,
        choice_arm: adt("narrative.kis", "ChoiceArm")?,
        setting: find("system.kis", "Setting")?,
        meta: find("narrative.kis", "Meta")?,
        push_step: find("narrative.kis", "push_step")?,
        rng_choose: find("narrative.kis", "rng_choose")?,
        ui_slider: find("ui.kis", "slider")?,
        time_file: *std_files.get("time.kis")?,
        assets,
        builtins,
    })
}

impl StdTable {
    pub fn builtin(&self, item: ItemId) -> Option<Builtin> {
        self.builtins.get(&item).copied()
    }

    pub fn asset_kind(&self, item: ItemId) -> Option<AssetKind> {
        self.assets
            .iter()
            .find(|(asset, _)| *asset == item)
            .map(|(_, kind)| *kind)
    }

    pub fn none(&self) -> ConstValue {
        self.option.unit("None")
    }

    pub fn some(&self, value: ConstValue) -> ConstValue {
        ConstValue::Adt {
            item: self.option.item,
            variant: self.option.variant("Some").unwrap_or(1),
            fields: imbl::vector![value],
        }
    }

    /// `Some(v)` → `Some(Some(v))`, `None` → `Some(None)`; anything else → `None`.
    pub fn as_option<'v>(&self, value: &'v ConstValue) -> Option<Option<&'v ConstValue>> {
        match value {
            ConstValue::Adt {
                item,
                variant,
                fields,
            } if *item == self.option.item => {
                if Some(*variant) == self.option.variant("Some") {
                    Some(fields.front())
                } else {
                    Some(None)
                }
            }
            _ => None,
        }
    }

    /// `Duration { ms }`.
    pub fn duration_value(&self, ms: u32) -> ConstValue {
        self.duration.make(vec![ConstValue::Int(i128::from(ms))])
    }

    /// The `ms` of a `Duration` value.
    pub fn duration_ms(&self, value: &ConstValue) -> Option<u32> {
        let index = self.duration.field("ms")?;
        u32::try_from(value.field(index)?.as_int()?).ok()
    }

    /// The `u32` of an `EntityId`, or the `u8` of a `Part` or `Mark`.
    pub fn newtype_int(&self, value: &ConstValue) -> Option<i128> {
        value.field(0)?.as_int()
    }

    pub fn entity_id_value(&self, id: u32) -> ConstValue {
        self.entity_id.make(vec![ConstValue::Int(i128::from(id))])
    }
}
