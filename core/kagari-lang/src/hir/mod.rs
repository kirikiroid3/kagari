//! High-level IR: signatures and bodies lowered from the AST with forms desugared
//! (compilation stage 1). Names are unresolved paths; `resolve` binds them.

mod forms;
pub mod lower;

use kagari_syntax::TextRange;
use la_arena::{Arena, ArenaMap, Idx};
use smol_str::SmolStr;

use crate::item_tree::ItemId;

pub use lower::{FileHir, file_hir};

pub type Name = SmolStr;
pub type ExprId = Idx<Expr>;
pub type PatId = Idx<Pat>;
pub type TypeRefId = Idx<TypeRef>;

/// Expressions, patterns and types of one body or signature, with their source ranges.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExprStore {
    pub exprs: Arena<Expr>,
    pub pats: Arena<Pat>,
    pub types: Arena<TypeRef>,
    pub expr_ranges: ArenaMap<ExprId, TextRange>,
    pub pat_ranges: ArenaMap<PatId, TextRange>,
    pub type_ranges: ArenaMap<TypeRefId, TextRange>,
}

impl ExprStore {
    pub fn alloc_expr(&mut self, expr: Expr, range: TextRange) -> ExprId {
        let id = self.exprs.alloc(expr);
        self.expr_ranges.insert(id, range);
        id
    }

    pub fn alloc_pat(&mut self, pat: Pat, range: TextRange) -> PatId {
        let id = self.pats.alloc(pat);
        self.pat_ranges.insert(id, range);
        id
    }

    pub fn alloc_type(&mut self, ty: TypeRef, range: TextRange) -> TypeRefId {
        let id = self.types.alloc(ty);
        self.type_ranges.insert(id, range);
        id
    }

    /// Ids always come from this store, so indexing never fails.
    pub fn expr(&self, id: ExprId) -> &Expr {
        &self.exprs[id]
    }

    pub fn pat(&self, id: PatId) -> &Pat {
        &self.pats[id]
    }

    pub fn type_ref(&self, id: TypeRefId) -> &TypeRef {
        &self.types[id]
    }

    pub fn expr_range(&self, id: ExprId) -> TextRange {
        self.expr_ranges.get(id).copied().unwrap_or_default()
    }

    pub fn pat_range(&self, id: PatId) -> TextRange {
        self.pat_ranges.get(id).copied().unwrap_or_default()
    }

    pub fn type_range(&self, id: TypeRefId) -> TextRange {
        self.type_ranges.get(id).copied().unwrap_or_default()
    }
}

/// A path as written, or synthesized by form desugaring (`PathKind::Lang`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Path {
    pub kind: PathKind,
    pub segments: Vec<PathSegment>,
}

impl Path {
    pub fn lang(item: LangItem) -> Path {
        Path {
            kind: PathKind::Lang(item),
            segments: Vec::new(),
        }
    }

    /// The path as written, e.g. `crate::a::B`, `super::super::x`, `Screen::empty`.
    pub fn display(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        match self.kind {
            PathKind::Plain => {}
            PathKind::Crate => parts.push("crate".to_owned()),
            PathKind::SelfModule => parts.push("self".to_owned()),
            PathKind::Super(count) => {
                for _ in 0..count {
                    parts.push("super".to_owned());
                }
            }
            PathKind::Lang(item) => {
                parts.push("std".to_owned());
                parts.extend(item.std_path().iter().map(|part| (*part).to_owned()));
            }
        }
        parts.extend(self.segments.iter().map(|segment| segment.name.to_string()));
        parts.join("::")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PathKind {
    Plain,
    Crate,
    SelfModule,
    Super(u32),
    Lang(LangItem),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PathSegment {
    pub name: Name,
    pub range: TextRange,
    pub args: Option<GenericArgs>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GenericArgs(pub Vec<GenericArg>);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GenericArg {
    Type(TypeRefId),
    Const(ExprId),
}

/// Std items that form desugaring refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LangItem {
    PushStep,
    Say,
    Choice,
    ChoiceArm,
    OptionSome,
    OptionNone,
    Ruby,
    MonoRuby,
    Effect,
    Concat,
    ToText,
    Asset,
    UiLabel,
    UiColumn,
    UiRow,
    UiLayer,
    UiText,
    UiTextProps,
    UiImage,
    UiImageProps,
    UiGameScreen,
    UiGameScreenProps,
    UiButton,
    UiButtonProps,
    UiSlider,
    UiSliderProps,
    UiToggle,
    UiToggleProps,
    UiSelect,
    UiSelectProps,
}

impl LangItem {
    /// The item's path relative to the std root.
    pub fn std_path(self) -> &'static [&'static str] {
        match self {
            LangItem::PushStep => &["narrative", "push_step"],
            LangItem::Say => &["narrative", "Say"],
            LangItem::Choice => &["narrative", "Choice"],
            LangItem::ChoiceArm => &["narrative", "ChoiceArm"],
            LangItem::OptionSome => &["Option", "Some"],
            LangItem::OptionNone => &["Option", "None"],
            LangItem::Ruby => &["narrative", "text", "ruby"],
            LangItem::MonoRuby => &["narrative", "text", "mono_ruby"],
            LangItem::Effect => &["narrative", "text", "effect"],
            LangItem::Concat => &["narrative", "text", "concat"],
            LangItem::ToText => &["narrative", "text", "to_text"],
            LangItem::Asset => &["asset"],
            LangItem::UiLabel => &["ui", "label"],
            LangItem::UiColumn => &["ui", "column"],
            LangItem::UiRow => &["ui", "row"],
            LangItem::UiLayer => &["ui", "layer"],
            LangItem::UiText => &["ui", "text"],
            LangItem::UiTextProps => &["ui", "TextProps"],
            LangItem::UiImage => &["ui", "image"],
            LangItem::UiImageProps => &["ui", "ImageProps"],
            LangItem::UiGameScreen => &["ui", "game_screen"],
            LangItem::UiGameScreenProps => &["ui", "GameScreenProps"],
            LangItem::UiButton => &["ui", "button"],
            LangItem::UiButtonProps => &["ui", "ButtonProps"],
            LangItem::UiSlider => &["ui", "slider"],
            LangItem::UiSliderProps => &["ui", "SliderProps"],
            LangItem::UiToggle => &["ui", "toggle"],
            LangItem::UiToggleProps => &["ui", "ToggleProps"],
            LangItem::UiSelect => &["ui", "select"],
            LangItem::UiSelectProps => &["ui", "SelectProps"],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeRef {
    Missing,
    Path(Path),
    Tuple(Vec<TypeRefId>),
    Array {
        elem: TypeRefId,
        len: ExprId,
    },
    Ref {
        mutable: bool,
        inner: TypeRefId,
    },
    FnPtr {
        kind: FnPtrKind,
        params: Vec<TypeRefId>,
        ret: Option<TypeRefId>,
    },
    Closure {
        kind: ClosureKind,
        params: Vec<TypeRefId>,
        ret: Option<TypeRefId>,
    },
    Infer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FnPtrKind {
    Plain,
    Const,
    Pushdown,
    Scene,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClosureKind {
    Plain,
    Const,
    Pushdown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pat {
    Missing,
    Wild,
    Bind {
        name: Name,
        mutable: bool,
    },
    Path(Path),
    Lit {
        lit: Literal,
        negated: bool,
    },
    Range {
        start: Option<PatBound>,
        end: Option<PatBound>,
        inclusive: bool,
    },
    Tuple(Vec<PatId>),
    TupleStruct {
        path: Path,
        pats: Vec<PatId>,
    },
    Record {
        path: Path,
        fields: Vec<(Name, PatId)>,
    },
    Or(Vec<PatId>),
    Rest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatBound {
    Lit { lit: Literal, negated: bool },
    Path(Path),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Literal {
    Int {
        value: u128,
        suffix: Option<SmolStr>,
    },
    Decimal(SmolStr),
    String(String),
    Bool(bool),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    Missing,
    Literal(Literal),
    Path(Path),
    Block {
        stmts: Vec<Stmt>,
        tail: Option<ExprId>,
        /// The item-tree block scope, when the block contains items.
        scope: Option<u32>,
    },
    If {
        cond: Condition,
        then_branch: ExprId,
        else_branch: Option<ExprId>,
    },
    Match {
        scrutinee: ExprId,
        arms: Vec<MatchArm>,
    },
    Loop {
        label: Option<Name>,
        body: ExprId,
    },
    While {
        label: Option<Name>,
        cond: Condition,
        body: ExprId,
    },
    For {
        label: Option<Name>,
        pat: PatId,
        iterable: ExprId,
        body: ExprId,
    },
    Break {
        label: Option<Name>,
        value: Option<ExprId>,
    },
    Continue {
        label: Option<Name>,
    },
    Return {
        value: Option<ExprId>,
    },
    Call {
        callee: ExprId,
        args: Vec<ExprId>,
    },
    MethodCall {
        receiver: ExprId,
        name: Name,
        args_generic: Option<GenericArgs>,
        args: Vec<ExprId>,
    },
    Field {
        base: ExprId,
        name: FieldName,
    },
    Index {
        base: ExprId,
        index: ExprId,
    },
    Try(ExprId),
    Cast {
        expr: ExprId,
        ty: TypeRefId,
    },
    Unary {
        op: UnaryOp,
        expr: ExprId,
    },
    Ref {
        mutable: bool,
        expr: ExprId,
    },
    Binary {
        op: BinaryOp,
        lhs: ExprId,
        rhs: ExprId,
    },
    Assign {
        /// `Some(op)` for compound assignment `op=`.
        op: Option<BinaryOp>,
        target: ExprId,
        value: ExprId,
    },
    Range {
        start: Option<ExprId>,
        end: Option<ExprId>,
        inclusive: bool,
    },
    Tuple(Vec<ExprId>),
    Array(ArrayExpr),
    Record {
        path: Path,
        fields: Vec<(Name, ExprId)>,
    },
    Closure {
        params: Vec<(PatId, Option<TypeRefId>)>,
        ret: Option<TypeRefId>,
        body: ExprId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Condition {
    Expr(ExprId),
    Let { pat: PatId, expr: ExprId },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatchArm {
    pub pat: PatId,
    pub guard: Option<ExprId>,
    pub body: ExprId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldName {
    Named(Name),
    Index(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    Neg,
    Not,
    Deref,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArrayExpr {
    List(Vec<ExprId>),
    Repeat { value: ExprId, count: ExprId },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stmt {
    Let {
        pat: PatId,
        ty: Option<TypeRefId>,
        init: Option<ExprId>,
    },
    Expr {
        expr: ExprId,
        has_semi: bool,
    },
    Item(ItemId),
}

/// The body of a `fn`, `const` or `static`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Body {
    pub store: ExprStore,
    pub params: Vec<PatId>,
    pub has_self: bool,
    /// Whether the `self` parameter is `mut self` (by value).
    pub self_mutable: bool,
    pub value: ExprId,
}

/// The signature of an item: generics plus the types it declares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    pub store: ExprStore,
    pub generics: Vec<GenericParam>,
    pub kind: SignatureKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenericParam {
    pub name: Name,
    pub range: TextRange,
    pub kind: GenericParamKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GenericParamKind {
    Type { bounds: Vec<TypeRefId> },
    Const { ty: TypeRefId },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SelfParamKind {
    Value,
    Ref,
    RefMut,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignatureKind {
    Fn {
        params: Vec<TypeRefId>,
        self_param: Option<SelfParamKind>,
        ret: Option<TypeRefId>,
    },
    Struct {
        fields: Vec<(Option<Name>, TypeRefId)>,
    },
    Enum {
        variants: Vec<Vec<(Option<Name>, TypeRefId)>>,
    },
    Const {
        ty: TypeRefId,
    },
    Static {
        ty: TypeRefId,
    },
    TypeAlias {
        bounds: Vec<TypeRefId>,
        value: Option<TypeRefId>,
    },
    Impl {
        trait_ref: Option<TypeRefId>,
        self_ty: TypeRefId,
    },
    Trait {
        bounds: Vec<TypeRefId>,
    },
    None,
}
