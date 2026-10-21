//! The system-scope HIR: settings, save slots, system functions and modes.

use la_arena::{Arena, Idx};
use smol_str::SmolStr;

use crate::diagnostic::Span;
use crate::kis::hir::{MatchArm, MatchStmt, MetaDecl, SceneDecl};
use crate::kis::value::{Value, VarType};

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SystemHir {
    /// The root page named by `<system start>`; `None` when it does not resolve.
    pub start: Option<Idx<ModeDecl>>,
    pub settings: Arena<SettingDecl>,
    pub save_slots: Option<SaveSlots>,
    pub sysfns: Arena<SysFnDecl>,
    pub locals: Arena<SysLocal>,
    pub modes: Arena<ModeDecl>,
    pub game_mode: Option<Idx<ModeDecl>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaveSlots {
    pub count: u16,
    pub span: Span,
}

/// The type of a system value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SysType {
    Var(VarType),
    /// Two `i16`: x grows to the right, y downward.
    Vec2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SysValue {
    Var(Value),
    Vec2(i16, i16),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingDecl {
    pub name: SmolStr,
    pub span: Span,
    pub ty: VarType,
    pub init: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SysFnDecl {
    pub name: SmolStr,
    pub span: Span,
    pub args: Vec<SysType>,
    pub returns: Option<SysType>,
    pub locals: Vec<Idx<SysLocal>>,
    pub body: Vec<SysStmt>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SysLocal {
    pub name: SmolStr,
    pub span: Span,
    pub ty: SysType,
    pub init: SysValue,
}

/// A variable readable from system code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SysVar {
    Local(Idx<SysLocal>),
    Setting(Idx<SettingDecl>),
    Meta(Idx<MetaDecl>),
    /// `slot.index` inside `<for-slots>`.
    SlotIndex,
    /// `choice.index` inside `<for-choices>`.
    ChoiceIndex,
    /// `entry.index` inside `<for-history>`.
    EntryIndex,
    /// `entry.voice` inside `<for-history>`.
    EntryVoice,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operand {
    Var(SysVar),
    Literal(SysValue),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SysStmt {
    pub kind: SysStmtKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SysStmtKind {
    Arg { position: u8, var: Idx<SysLocal> },
    Return(Option<Idx<SysLocal>>),
    Call(SysCall),
    Op(SysOp),
    Match(MatchStmt<Vec<SysStmt>, SysVar>),
    Set { var: SysVar, value: SysValue },
    Add { var: SysVar, amount: u16 },
    Sub { var: SysVar, amount: u16 },
    MetaSet { meta: Idx<MetaDecl>, value: Value },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SysCall {
    pub func: Idx<SysFnDecl>,
    pub args: Vec<Operand>,
    pub save: Option<SysVar>,
}

/// A meta-function or interface operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SysOp {
    Start(Idx<SceneDecl>),
    Load(Operand),
    Save(Operand),
    Advance,
    Select(Operand),
    Quit,
    /// `metafn__revert(entry.index)`.
    Revert,
    /// `metafn__play_voice(entry.voice)`.
    PlayVoice,
    Push(Idx<ModeDecl>),
    Pop,
    Goto(Idx<ModeDecl>),
    FocusMove(FocusTarget),
    FocusActivate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusTarget {
    Direction(Operand),
    Index(Operand),
    Next,
    Prev,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeKind {
    RootPage,
    Page,
    Dialog,
    Game,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModeDecl {
    pub name: SmolStr,
    pub span: Span,
    pub kind: ModeKind,
    pub requires_game: bool,
    pub on_keyboard: Option<SysStmt>,
    pub on_controller: Option<SysStmt>,
    pub ui: Vec<UiNode>,
    /// The `<on-exit>` arms of the game mode, matched against the game's `Ending`.
    pub on_exit: Vec<MatchArm<Vec<SysStmt>>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiNode {
    pub kind: UiKind,
    pub span: Span,
    pub style: Option<SmolStr>,
    pub on_click: Option<SysStmt>,
    pub bind: Option<Idx<SettingDecl>>,
    pub children: Vec<UiNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiKind {
    Layer(SmolStr),
    GameScreen,
    /// The label text.
    Button(String),
    Slider,
    Toggle,
    Select,
    Row,
    Col,
    TextFrame,
    BacklogLine,
    ForSlots,
    ForHistory,
    ForChoices,
}
