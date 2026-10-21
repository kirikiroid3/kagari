//! The HIR of both scopes produced by stage 2: the game's scenes and variables, the shared
//! enums and meta fields, and the system scope (see [`crate::kis::system_hir`]).

use kagari_syntax::TextRange;
use la_arena::{Arena, Idx};
use smol_str::SmolStr;

use crate::diagnostic::{Diagnostic, Span};
use crate::input::SourceFile;
use crate::kis::system_hir::SystemHir;
use crate::kis::value::{Pattern, Value, VarType};

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ProgramHir {
    pub enums: Arena<EnumDecl>,
    pub vars: Arena<VarDecl>,
    pub metas: Arena<MetaDecl>,
    pub scenes: Arena<SceneDecl>,
    pub renders: Arena<Render>,
    pub system: SystemHir,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnumDecl {
    pub name: SmolStr,
    pub span: Span,
    pub variants: Vec<VariantDecl>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariantDecl {
    pub name: SmolStr,
    pub repr: u8,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VarDecl {
    pub name: SmolStr,
    pub span: Span,
    pub ty: VarType,
    pub init: Value,
    pub scope: VarScope,
    pub from_meta: Option<Idx<MetaDecl>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VarScope {
    Global,
    Local(Idx<SceneDecl>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetaDecl {
    pub name: SmolStr,
    pub span: Span,
    pub ty: VarType,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneDecl {
    pub name: SmolStr,
    pub span: Span,
    /// The type of the returned value; `None` for a scene that returns no value, or whose
    /// `returns` is invalid.
    pub returns: Option<VarType>,
    /// Whether the system can start the story at this scene (`entry="true"`).
    pub entry: bool,
    pub locals: Vec<Idx<VarDecl>>,
    pub labels: Vec<LabelDecl>,
    pub body: Block,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabelDecl {
    pub id: SmolStr,
    pub span: Span,
    pub block: BlockId,
}

/// Unique per scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(pub u32);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub id: BlockId,
    pub stmts: Vec<Stmt>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
    /// Spans of the enclosing `<x>` uses, innermost first.
    pub expansion: Vec<Span>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StmtKind {
    Say {
        messages: Vec<Message>,
        renders: Vec<Idx<Render>>,
    },
    Choice {
        id: SmolStr,
        renders: Vec<Idx<Render>>,
        items: Vec<ChoiceItem>,
    },
    /// Index into `SceneDecl::labels`.
    Label(u32),
    /// Index into `SceneDecl::labels`; `None` when unresolved.
    Goto(Option<u32>),
    Call {
        /// `None` when the scene is unknown.
        scene: Option<Idx<SceneDecl>>,
        /// The variable the returned value is assigned to.
        assign: Option<Idx<VarDecl>>,
    },
    /// `None` for a `<return>` without a value.
    Return(Option<ReturnValue>),
    Match(MatchStmt<Block>),
    Set {
        var: Idx<VarDecl>,
        value: Value,
    },
    Add {
        var: Idx<VarDecl>,
        amount: u16,
    },
    Sub {
        var: Idx<VarDecl>,
        amount: u16,
    },
    MetaSet {
        meta: Idx<MetaDecl>,
        value: Value,
    },
}

/// The value a `<return>` passes to the caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReturnValue {
    /// A local of the returning scene.
    Var(Idx<VarDecl>),
    Literal(Value),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchStmt<B, V = Idx<VarDecl>> {
    pub vars: Vec<V>,
    pub arms: Vec<MatchArm<B>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchArm<B> {
    /// One row per `<case>`; each row has one pattern per matched variable.
    pub rows: Vec<Vec<Pattern>>,
    pub body: B,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChoiceItem {
    Arm(ChoiceArm),
    Match(MatchStmt<Vec<ChoiceItem>>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChoiceArm {
    pub id: SmolStr,
    pub text: String,
    pub span: Span,
    pub body: Block,
}

/// An `<m>` inside an `<s>`. An empty `who` is narration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub who: String,
    pub voice: Option<(String, Span)>,
    pub text: Vec<TextPiece>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextPiece {
    pub text: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Render {
    /// An inline `<r>` element; `range` covers the whole element.
    Inline { file: SourceFile, range: TextRange },
    /// A `<represent src>` file.
    File { kirp: SourceFile, span: Span },
}
