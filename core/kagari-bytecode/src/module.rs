//! The in-memory form of a `.kagari` module: one field per section, one type per record.
//! Indices into the tables are `u32` newtypes; [`NONE`] marks an absent index in the file.

use crate::instr::Instr;

/// The 16-byte header: `\0\0\0\0KAGARI\0\0V000`.
pub const MAGIC: [u8; 16] = *b"\0\0\0\0KAGARI\0\0V000";

/// An absent index in the file.
pub const NONE: u32 = u32::MAX;

macro_rules! index {
    ($($(#[$doc:meta])* $name:ident;)*) => {
        $(
            $(#[$doc])*
            #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
            pub struct $name(pub u32);
        )*
    };
}

index! {
    /// An index into [`Module::strings`].
    StrId;
    /// An index into [`Module::types`].
    TypeId;
    /// An index into [`Module::int_types`].
    IntTypeId;
    /// An index into [`Module::imports`].
    ImportId;
    /// An index into [`Module::assets`].
    AssetId;
    /// An index into [`Module::constants`].
    ConstId;
    /// An index into [`Module::statics`].
    StaticId;
    /// An index into [`Module::functions`].
    FuncId;
}

/// A whole module.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Module {
    pub strings: Vec<String>,
    pub features: Vec<Feature>,
    pub types: Vec<AdtType>,
    pub int_types: Vec<IntType>,
    pub imports: Vec<Import>,
    pub assets: Vec<Asset>,
    pub constants: Vec<Constant>,
    pub statics: Vec<Static>,
    pub functions: Vec<Function>,
    pub options: Options,
    pub debug: Option<Debug>,
}

/// An engine extension the module uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Feature {
    pub name: StrId,
    /// An engine that lacks a required feature refuses to load the module.
    pub required: bool,
}

/// A struct (one variant) or an enum.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdtType {
    pub path: StrId,
    pub is_enum: bool,
    pub variants: Vec<Variant>,
}

/// A variant; a struct's single variant is named like the struct.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variant {
    pub name: StrId,
    /// Field names in declaration order; `None` for positional fields.
    pub fields: Vec<Option<StrId>>,
}

/// An integer type: the range `lo..=hi` and the bit width and signedness that shifts and
/// bitwise operations use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IntType {
    pub lo: i128,
    pub hi: i128,
    pub bits: u32,
    pub signed: bool,
}

impl IntType {
    /// Entry 0 of every module: `i128`.
    pub const I128: IntType = IntType {
        lo: i128::MIN,
        hi: i128::MAX,
        bits: 128,
        signed: true,
    };
}

/// The kind of a function: which functions it may call and which instructions it may use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FnKind {
    /// `fn`.
    Fn,
    /// `const fn`.
    Const,
    /// An instant `pushdown fn`.
    Instant,
    /// A scene.
    Scene,
}

impl FnKind {
    pub fn code(self) -> u8 {
        match self {
            FnKind::Fn => 0,
            FnKind::Const => 1,
            FnKind::Instant => 2,
            FnKind::Scene => 3,
        }
    }

    pub fn from_code(code: u8) -> Option<FnKind> {
        Some(match code {
            0 => FnKind::Fn,
            1 => FnKind::Const,
            2 => FnKind::Instant,
            3 => FnKind::Scene,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            FnKind::Fn => "fn",
            FnKind::Const => "const",
            FnKind::Instant => "instant",
            FnKind::Scene => "scene",
        }
    }

    /// Whether a function of this kind may call one of kind `callee`.
    pub fn may_call(self, callee: FnKind) -> bool {
        matches!(
            (self, callee),
            (FnKind::Fn, FnKind::Fn | FnKind::Const)
                | (FnKind::Const, FnKind::Const)
                | (FnKind::Instant, FnKind::Instant | FnKind::Const)
                | (
                    FnKind::Scene,
                    FnKind::Scene | FnKind::Instant | FnKind::Const
                )
        )
    }
}

/// A function the engine provides, called with `CALLIMP`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    /// The std path, e.g. `std::narrative::Meta::write`.
    pub path: StrId,
    /// Never [`FnKind::Scene`].
    pub kind: FnKind,
    pub params: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AssetKind {
    Image,
    Audio,
    Voice,
    Video,
    Font,
    Shader,
}

impl AssetKind {
    pub fn code(self) -> u32 {
        match self {
            AssetKind::Image => 0,
            AssetKind::Audio => 1,
            AssetKind::Voice => 2,
            AssetKind::Video => 3,
            AssetKind::Font => 4,
            AssetKind::Shader => 5,
        }
    }

    pub fn from_code(code: u32) -> Option<AssetKind> {
        Some(match code {
            0 => AssetKind::Image,
            1 => AssetKind::Audio,
            2 => AssetKind::Voice,
            3 => AssetKind::Video,
            4 => AssetKind::Font,
            5 => AssetKind::Shader,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            AssetKind::Image => "image",
            AssetKind::Audio => "audio",
            AssetKind::Voice => "voice",
            AssetKind::Video => "video",
            AssetKind::Font => "font",
            AssetKind::Shader => "shader",
        }
    }
}

/// An asset file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    pub kind: AssetKind,
    /// `/`-separated, relative to the root file's directory.
    pub path: StrId,
}

/// A constant. A constant refers only to constants with a lower index.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Constant {
    Bool(bool),
    Int(i128),
    /// `mantissa / 10^scale`.
    Decimal {
        mantissa: i128,
        scale: u32,
    },
    Rat {
        num: i128,
        den: i128,
    },
    String(StrId),
    /// Plain text.
    Text(StrId),
    /// Text with a ruby reading; both are Text constants.
    Ruby {
        base: ConstId,
        reading: ConstId,
    },
    /// Text with one reading per character.
    MonoRuby {
        base: ConstId,
        readings: Vec<ConstId>,
    },
    /// Text with a `TextEffect` value.
    Effect {
        base: ConstId,
        effect: ConstId,
    },
    Concat(Vec<ConstId>),
    /// Unit is the tuple of 0 items.
    Tuple(Vec<ConstId>),
    Array(Vec<ConstId>),
    Vec(Vec<ConstId>),
    Adt {
        ty: TypeId,
        variant: u32,
        fields: Vec<ConstId>,
    },
    Asset(AssetId),
    Function(FuncId),
    /// A `Setting` or `Meta` handle: the static it is the value of.
    Handle(StaticId),
}

impl Constant {
    /// The tag of this constant in the file.
    pub fn tag(&self) -> u32 {
        match self {
            Constant::Bool(_) => 0,
            Constant::Int(_) => 1,
            Constant::Decimal { .. } => 2,
            Constant::Rat { .. } => 3,
            Constant::String(_) => 4,
            Constant::Text(_) => 5,
            Constant::Ruby { .. } => 6,
            Constant::MonoRuby { .. } => 7,
            Constant::Effect { .. } => 8,
            Constant::Concat(_) => 9,
            Constant::Tuple(_) => 10,
            Constant::Array(_) => 11,
            Constant::Vec(_) => 12,
            Constant::Adt { .. } => 13,
            Constant::Asset(_) => 14,
            Constant::Function(_) => 15,
            Constant::Handle(_) => 16,
        }
    }

    /// The constants this one refers to.
    pub fn children(&self) -> Vec<ConstId> {
        match self {
            Constant::Ruby { base, reading } => vec![*base, *reading],
            Constant::MonoRuby { base, readings } => {
                let mut out = vec![*base];
                out.extend(readings.iter().copied());
                out
            }
            Constant::Effect { base, effect } => vec![*base, *effect],
            Constant::Concat(items)
            | Constant::Tuple(items)
            | Constant::Array(items)
            | Constant::Vec(items)
            | Constant::Adt { fields: items, .. } => items.clone(),
            _ => Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StaticKind {
    /// A `static mut` of the story, saved with the game.
    StoryVariable,
    /// A `Setting` handle's static.
    Setting,
    /// A `Meta` handle's static.
    Meta,
}

impl StaticKind {
    pub fn code(self) -> u32 {
        match self {
            StaticKind::StoryVariable => 0,
            StaticKind::Setting => 1,
            StaticKind::Meta => 2,
        }
    }

    pub fn from_code(code: u32) -> Option<StaticKind> {
        Some(match code {
            0 => StaticKind::StoryVariable,
            1 => StaticKind::Setting,
            2 => StaticKind::Meta,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            StaticKind::StoryVariable => "story",
            StaticKind::Setting => "setting",
            StaticKind::Meta => "meta",
        }
    }
}

/// A story variable, setting or meta field with its initial value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Static {
    pub path: StrId,
    pub kind: StaticKind,
    pub init: ConstId,
}

/// The flags of a function.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct FnFlags {
    /// An `#[entry]` scene.
    pub entry: bool,
    /// `fn main()` of the root module: what the engine runs at boot.
    pub main: bool,
    /// A closure body: its first parameters are the captures.
    pub closure: bool,
    /// A constructor, builtin wrapper or constant initializer made by the compiler.
    pub synthetic: bool,
}

impl FnFlags {
    pub fn bits(self) -> u8 {
        [
            (self.entry, 1),
            (self.main, 2),
            (self.closure, 4),
            (self.synthetic, 8),
        ]
        .iter()
        .filter(|(set, _)| *set)
        .fold(0, |bits, (_, bit)| bits | bit)
    }

    /// `None` when an unknown bit is set.
    pub fn from_bits(bits: u8) -> Option<FnFlags> {
        if bits & !0b1111 != 0 {
            return None;
        }
        Some(FnFlags {
            entry: bits & 1 != 0,
            main: bits & 2 != 0,
            closure: bits & 4 != 0,
            synthetic: bits & 8 != 0,
        })
    }
}

/// A function with its code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    pub path: StrId,
    pub kind: FnKind,
    pub flags: FnFlags,
    /// Parameters arrive in `r0..params`.
    pub params: u16,
    /// Registers `r0..regs`; every register starts `Uninit`.
    pub regs: u16,
    pub code: Vec<Instr>,
}

/// Run-time limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Options {
    /// The most nested calls at run time; deeper is a VM panic.
    pub max_call_depth: u32,
}

/// Source locations (the ancillary Debug section).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Debug {
    /// Source file paths.
    pub files: Vec<StrId>,
    /// One entry per function, in function order.
    pub functions: Vec<FnDebug>,
}

/// The source locations of one function's code.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct FnDebug {
    /// An index into [`Debug::files`]; `None` for code without a source.
    pub file: Option<u32>,
    /// `(code word index, byte offset in the file)`, ascending by word.
    pub lines: Vec<(u32, u32)>,
}
