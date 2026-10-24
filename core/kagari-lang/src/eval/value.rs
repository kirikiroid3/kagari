//! Values of const evaluation as plain data: what constants, folded expressions, tables and
//! screens hold.

use std::fmt::Write as _;

use crate::db::Db;
use crate::hir::ExprId;
use crate::input::FilePath;
use crate::item_tree::{ItemId, ItemKind, Shape, item_tree};
use crate::ty::{FnDefId, FnRef};

use super::num::{Decimal, Rat};
use super::screen::Screen;

/// A fully evaluated value. Plain data: no references, no captured environments.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ConstValue {
    Bool(bool),
    /// Every integer type: `Int<lo, hi>` and the Rust integers.
    Int(i128),
    Decimal(Decimal),
    Rat(Rat),
    Text(Text),
    String(String),
    /// Tuples, `()` included.
    Tuple(Vec<ConstValue>),
    Array(Vec<ConstValue>),
    Vec(Vec<ConstValue>),
    /// A struct (variant 0) or an enum variant, user-declared or from std; fields in
    /// declaration order.
    Adt {
        item: ItemId,
        variant: u32,
        fields: Vec<ConstValue>,
    },
    /// An asset constant: its kind and the file it names.
    Asset {
        kind: AssetKind,
        path: FilePath,
    },
    /// A named function or tuple constructor used as a value.
    Fn(FnRef),
    /// A closure that captures nothing: the item whose body contains it and the closure
    /// expression.
    Closure {
        owner: ItemId,
        expr: ExprId,
    },
    /// A `Setting<T>` or `Meta<T>` handle: the immutable `static` it is the value of.
    Handle {
        kind: HandleKind,
        item: ItemId,
    },
    Screen(Box<Screen>),
}

impl ConstValue {
    pub fn unit() -> ConstValue {
        ConstValue::Tuple(Vec::new())
    }

    pub fn as_int(&self) -> Option<i128> {
        match self {
            ConstValue::Int(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_decimal(&self) -> Option<Decimal> {
        match self {
            ConstValue::Decimal(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            ConstValue::Bool(value) => Some(*value),
            _ => None,
        }
    }

    /// Field `index` of a struct, variant or tuple.
    pub fn field(&self, index: usize) -> Option<&ConstValue> {
        match self {
            ConstValue::Adt { fields, .. } | ConstValue::Tuple(fields) => fields.get(index),
            _ => None,
        }
    }

    /// Visits every value inside `self`, `self` included, outermost first.
    pub fn walk(&self, visit: &mut dyn FnMut(&ConstValue)) {
        visit(self);
        match self {
            ConstValue::Tuple(items)
            | ConstValue::Array(items)
            | ConstValue::Vec(items)
            | ConstValue::Adt { fields: items, .. } => {
                for item in items {
                    item.walk(visit);
                }
            }
            ConstValue::Text(text) => text.walk_effects(&mut |effect| effect.walk(visit)),
            _ => {}
        }
    }
}

/// The six asset types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AssetKind {
    Image,
    Audio,
    Voice,
    Video,
    Font,
    Shader,
}

impl AssetKind {
    pub const ALL: [AssetKind; 6] = [
        AssetKind::Image,
        AssetKind::Audio,
        AssetKind::Voice,
        AssetKind::Video,
        AssetKind::Font,
        AssetKind::Shader,
    ];

    /// The std type name, e.g. `ImageAsset`.
    pub fn type_name(self) -> &'static str {
        match self {
            AssetKind::Image => "ImageAsset",
            AssetKind::Audio => "AudioAsset",
            AssetKind::Voice => "VoiceAsset",
            AssetKind::Video => "VideoAsset",
            AssetKind::Font => "FontAsset",
            AssetKind::Shader => "ShaderAsset",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HandleKind {
    Setting,
    Meta,
}

/// A constant text: plain, or rich as built by `t![…]`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Text {
    Plain(String),
    /// `ruby(base, reading)`.
    Ruby {
        base: Box<Text>,
        reading: Box<Text>,
    },
    /// `mono_ruby(base, readings)`.
    MonoRuby {
        base: Box<Text>,
        readings: Vec<Text>,
    },
    /// `effect(base, effect)`; `effect` is the `TextEffect` value.
    Effect {
        base: Box<Text>,
        effect: Box<ConstValue>,
    },
    /// `concat(items)`.
    Concat(Vec<Text>),
}

impl Text {
    pub fn plain(text: impl Into<String>) -> Text {
        Text::Plain(text.into())
    }

    /// The characters shown, without readings or effects.
    pub fn base_string(&self) -> String {
        let mut out = String::new();
        self.push_base(&mut out);
        out
    }

    fn push_base(&self, out: &mut String) {
        match self {
            Text::Plain(text) => out.push_str(text),
            Text::Ruby { base, .. } | Text::MonoRuby { base, .. } | Text::Effect { base, .. } => {
                base.push_base(out);
            }
            Text::Concat(items) => {
                for item in items {
                    item.push_base(out);
                }
            }
        }
    }

    /// Visits the `TextEffect` value of every effect in the text, in reading order.
    pub fn walk_effects(&self, visit: &mut dyn FnMut(&ConstValue)) {
        match self {
            Text::Plain(_) => {}
            Text::Ruby { base, reading } => {
                base.walk_effects(visit);
                reading.walk_effects(visit);
            }
            Text::MonoRuby { base, readings } => {
                base.walk_effects(visit);
                for reading in readings {
                    reading.walk_effects(visit);
                }
            }
            Text::Effect { base, effect } => {
                visit(effect);
                base.walk_effects(visit);
            }
            Text::Concat(items) => {
                for item in items {
                    item.walk_effects(visit);
                }
            }
        }
    }
}

/// The name of an item, or `?`.
pub(crate) fn item_name(db: &dyn Db, item: ItemId) -> String {
    item_tree(db, item.file)
        .item(item.index)
        .and_then(|data| data.name.clone())
        .map_or_else(|| "?".to_owned(), |name| name.to_string())
}

/// The variant names and shapes of an enum, or the single shape of a struct (named like the
/// struct).
pub(crate) fn adt_variants(db: &dyn Db, item: ItemId) -> Vec<(String, Shape)> {
    let Some(data) = item_tree(db, item.file).item(item.index) else {
        return Vec::new();
    };
    match &data.kind {
        ItemKind::Struct { shape } => vec![(item_name(db, item), shape.clone())],
        ItemKind::Enum { variants } => variants
            .iter()
            .map(|(name, _, shape)| (name.to_string(), shape.clone()))
            .collect(),
        _ => Vec::new(),
    }
}

/// A value as it would be written in Kagari, for diagnostics: `Mood::Warm`, `Some(3)`,
/// `Point { x: 1, y: 2 }`, `"Mio"`.
pub fn display_value(db: &dyn Db, value: &ConstValue) -> String {
    let mut out = String::new();
    write_value(db, value, &mut out);
    out
}

fn write_list(db: &dyn Db, items: &[ConstValue], out: &mut String) {
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        write_value(db, item, out);
    }
}

fn write_value(db: &dyn Db, value: &ConstValue, out: &mut String) {
    match value {
        ConstValue::Bool(value) => {
            let _ = write!(out, "{value}");
        }
        ConstValue::Int(value) => {
            let _ = write!(out, "{value}");
        }
        ConstValue::Decimal(value) => {
            let _ = write!(out, "{value}");
        }
        ConstValue::Rat(value) => {
            let _ = write!(out, "{value}");
        }
        ConstValue::Text(text) => {
            let _ = write!(out, "{:?}", text.base_string());
        }
        ConstValue::String(text) => {
            let _ = write!(out, "{text:?}");
        }
        ConstValue::Tuple(items) => {
            out.push('(');
            write_list(db, items, out);
            if items.len() == 1 {
                out.push(',');
            }
            out.push(')');
        }
        ConstValue::Array(items) => {
            out.push('[');
            write_list(db, items, out);
            out.push(']');
        }
        ConstValue::Vec(items) => {
            out.push_str("Vec[");
            write_list(db, items, out);
            out.push(']');
        }
        ConstValue::Adt {
            item,
            variant,
            fields,
        } => {
            let variants = adt_variants(db, *item);
            let is_enum = matches!(
                item_tree(db, item.file)
                    .item(item.index)
                    .map(|data| &data.kind),
                Some(ItemKind::Enum { .. })
            );
            let (name, shape) = variants
                .get(*variant as usize)
                .cloned()
                .unwrap_or_else(|| ("?".to_owned(), Shape::Tuple(0)));
            // `Some`, `None`, `Ok` and `Err` are in the prelude; other variants are qualified.
            let prelude = matches!(name.as_str(), "Some" | "None" | "Ok" | "Err");
            if is_enum && !prelude {
                let _ = write!(out, "{}::", item_name(db, *item));
            }
            out.push_str(&name);
            match shape {
                Shape::Unit => {}
                Shape::Tuple(_) => {
                    out.push('(');
                    write_list(db, fields, out);
                    out.push(')');
                }
                Shape::Record(names) => {
                    out.push_str(" { ");
                    for (index, (field, value)) in names.iter().zip(fields).enumerate() {
                        if index > 0 {
                            out.push_str(", ");
                        }
                        let _ = write!(out, "{field}: ");
                        write_value(db, value, out);
                    }
                    out.push_str(" }");
                }
            }
        }
        ConstValue::Asset { kind, path } => {
            let _ = write!(out, "{}({:?})", kind.type_name(), path.display(None));
        }
        ConstValue::Fn(fn_ref) => match fn_ref.def {
            FnDefId::Item(item) | FnDefId::StructCtor(item) => {
                out.push_str(&item_name(db, item));
            }
            FnDefId::VariantCtor(item, index) => {
                let name = adt_variants(db, item)
                    .get(index as usize)
                    .map_or_else(|| "?".to_owned(), |(name, _)| name.clone());
                let _ = write!(out, "{}::{name}", item_name(db, item));
            }
        },
        ConstValue::Closure { .. } => out.push_str("<closure>"),
        ConstValue::Handle { item, .. } => out.push_str(&item_name(db, *item)),
        ConstValue::Screen(_) => out.push_str("<screen>"),
    }
}
