//! Typed wrappers over the `.kirp`/`.kix` syntax tree.
//!
//! Every accessor returns `Option` or an iterator: the tree may come from erroneous input.

use rowan::TextRange;
use rowan::ast::{AstChildren, AstNode, support};

use crate::represent::cst::{RepresentKind, RepresentLanguage, SyntaxNode, SyntaxToken};
use crate::represent::lex::text_size;

macro_rules! ast_node {
    ($(#[$meta:meta])* $name:ident, $kind:ident) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub struct $name(SyntaxNode);

        impl AstNode for $name {
            type Language = RepresentLanguage;

            fn can_cast(kind: RepresentKind) -> bool {
                kind == RepresentKind::$kind
            }

            fn cast(node: SyntaxNode) -> Option<Self> {
                Self::can_cast(node.kind()).then(|| $name(node))
            }

            fn syntax(&self) -> &SyntaxNode {
                &self.0
            }
        }
    };
}

macro_rules! ast_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident($kind:ident)),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub enum $name {
            $($variant($variant),)*
        }

        impl AstNode for $name {
            type Language = RepresentLanguage;

            fn can_cast(kind: RepresentKind) -> bool {
                matches!(kind, $(RepresentKind::$kind)|*)
            }

            fn cast(node: SyntaxNode) -> Option<Self> {
                Some(match node.kind() {
                    $(RepresentKind::$kind => $name::$variant($variant(node)),)*
                    _ => return None,
                })
            }

            fn syntax(&self) -> &SyntaxNode {
                match self {
                    $($name::$variant(it) => it.syntax(),)*
                }
            }
        }

        $(
            impl From<$variant> for $name {
                fn from(it: $variant) -> $name {
                    $name::$variant(it)
                }
            }
        )*
    };
}

fn tokens(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> + use<> {
    node.children_with_tokens().filter_map(|it| it.into_token())
}

fn token_text(token: Option<SyntaxToken>) -> String {
    token.map(|t| t.text().to_owned()).unwrap_or_default()
}

// ---- roots --------------------------------------------------------------------------------

ast_node!(
    /// A whole `.kirp`/`.kix` file.
    SourceFile, SOURCE_FILE
);
ast_node!(
    /// The statements of an inline `<r>` element.
    InlineRender, INLINE_RENDER
);

impl SourceFile {
    pub fn items(&self) -> AstChildren<Item> {
        support::children(&self.0)
    }
}

impl InlineRender {
    pub fn stmts(&self) -> AstChildren<Stmt> {
        support::children(&self.0)
    }
}

// ---- items --------------------------------------------------------------------------------

ast_enum!(
    Item {
        Fn(FN),
        Struct(STRUCT),
        Enum(ENUM),
        TypeAlias(TYPE_ALIAS),
        Const(CONST),
        Mod(MOD),
        Impl(IMPL),
        AssetImport(ASSET_IMPORT),
    }
);

/// Items carry their leading `#[…]` attributes as children.
pub trait HasAttrs: AstNode<Language = RepresentLanguage> {
    fn attrs(&self) -> AstChildren<Attr> {
        support::children(self.syntax())
    }

    /// Whether an attribute with this name is present.
    fn has_attr(&self, name: &str) -> bool {
        self.attrs()
            .any(|attr| attr.name_text().as_deref() == Some(name))
    }
}

/// Nodes with a defining `NAME` child.
pub trait HasName: AstNode<Language = RepresentLanguage> {
    fn name(&self) -> Option<Name> {
        support::child(self.syntax())
    }
}

/// Nodes with a `GENERIC_PARAM_LIST` child.
pub trait HasGenericParams: AstNode<Language = RepresentLanguage> {
    fn generic_param_list(&self) -> Option<GenericParamList> {
        support::child(self.syntax())
    }

    /// The generic parameters, or nothing if there is no list.
    fn generic_params(
        &self,
    ) -> std::iter::Flatten<std::option::IntoIter<AstChildren<GenericParam>>> {
        self.generic_param_list()
            .map(|list| list.params())
            .into_iter()
            .flatten()
    }
}

impl HasAttrs for Item {}
impl HasName for Item {}

macro_rules! impl_traits {
    ($($trait:ident for $($ty:ident),*;)*) => {
        $($(impl $trait for $ty {})*)*
    };
}

impl_traits! {
    HasAttrs for Fn, Struct, Enum, TypeAlias, Const, Mod, Impl, AssetImport;
    HasName for Fn, Struct, Enum, TypeAlias, Const, Mod, AssetImport, RecordField, Variant, GenericParam;
    HasGenericParams for Fn, Struct, Enum, TypeAlias, Impl;
}

ast_node!(
    /// `#[name]` or `#[name(…)]`.
    Attr, ATTR
);
ast_node!(
    /// The balanced `( … )` arguments of an attribute.
    TokenTree, TOKEN_TREE
);

impl Attr {
    pub fn name_ref(&self) -> Option<NameRef> {
        support::child(&self.0)
    }

    pub fn name_text(&self) -> Option<String> {
        self.name_ref().map(|name| name.text())
    }

    pub fn token_tree(&self) -> Option<TokenTree> {
        support::child(&self.0)
    }

    pub fn has_args(&self) -> bool {
        self.token_tree().is_some()
    }
}

ast_node!(Fn, FN);
ast_node!(ParamList, PARAM_LIST);
ast_node!(
    /// The `self` receiver.
    SelfParam, SELF_PARAM
);
ast_node!(Param, PARAM);
ast_node!(RetType, RET_TYPE);

impl Fn {
    pub fn fn_token(&self) -> Option<SyntaxToken> {
        support::token(&self.0, RepresentKind::FN_KW)
    }

    pub fn param_list(&self) -> Option<ParamList> {
        support::child(&self.0)
    }

    pub fn ret_type(&self) -> Option<RetType> {
        support::child(&self.0)
    }

    pub fn body(&self) -> Option<BlockExpr> {
        support::child(&self.0)
    }

    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.0, RepresentKind::SEMICOLON)
    }
}

impl ParamList {
    pub fn self_param(&self) -> Option<SelfParam> {
        support::child(&self.0)
    }

    pub fn params(&self) -> AstChildren<Param> {
        support::children(&self.0)
    }
}

impl SelfParam {
    pub fn self_token(&self) -> Option<SyntaxToken> {
        support::token(&self.0, RepresentKind::SELF_KW)
    }
}

impl Param {
    pub fn pat(&self) -> Option<Pat> {
        support::child(&self.0)
    }

    pub fn ty(&self) -> Option<Type> {
        support::child(&self.0)
    }
}

impl RetType {
    pub fn ty(&self) -> Option<Type> {
        support::child(&self.0)
    }
}

ast_node!(Struct, STRUCT);
ast_node!(RecordFieldList, RECORD_FIELD_LIST);
ast_node!(RecordField, RECORD_FIELD);

impl Struct {
    /// `None` for a unit struct (`struct Unit;`).
    pub fn record_field_list(&self) -> Option<RecordFieldList> {
        support::child(&self.0)
    }

    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.0, RepresentKind::SEMICOLON)
    }
}

impl RecordFieldList {
    pub fn fields(&self) -> AstChildren<RecordField> {
        support::children(&self.0)
    }
}

impl RecordField {
    pub fn ty(&self) -> Option<Type> {
        support::child(&self.0)
    }
}

ast_node!(Enum, ENUM);
ast_node!(VariantList, VARIANT_LIST);
ast_node!(Variant, VARIANT);
ast_node!(TupleFieldList, TUPLE_FIELD_LIST);

impl Enum {
    pub fn variant_list(&self) -> Option<VariantList> {
        support::child(&self.0)
    }

    pub fn variants(&self) -> std::iter::Flatten<std::option::IntoIter<AstChildren<Variant>>> {
        self.variant_list()
            .map(|list| list.variants())
            .into_iter()
            .flatten()
    }
}

impl VariantList {
    pub fn variants(&self) -> AstChildren<Variant> {
        support::children(&self.0)
    }
}

impl Variant {
    /// `None` for a unit variant.
    pub fn tuple_field_list(&self) -> Option<TupleFieldList> {
        support::child(&self.0)
    }
}

impl TupleFieldList {
    pub fn types(&self) -> AstChildren<Type> {
        support::children(&self.0)
    }
}

ast_node!(TypeAlias, TYPE_ALIAS);
ast_node!(Const, CONST);
ast_node!(Mod, MOD);
ast_node!(Impl, IMPL);
ast_node!(
    /// The `{ Item* }` body of a `mod` or `impl`.
    ItemList, ITEM_LIST
);
ast_node!(
    /// `import NAME from "path" as Type;`
    AssetImport, ASSET_IMPORT
);

impl TypeAlias {
    pub fn ty(&self) -> Option<Type> {
        support::child(&self.0)
    }
}

impl Const {
    pub fn ty(&self) -> Option<Type> {
        support::child(&self.0)
    }

    /// The initializer expression.
    pub fn body(&self) -> Option<Expr> {
        support::child(&self.0)
    }
}

impl Mod {
    pub fn item_list(&self) -> Option<ItemList> {
        support::child(&self.0)
    }

    pub fn items(&self) -> std::iter::Flatten<std::option::IntoIter<AstChildren<Item>>> {
        self.item_list()
            .map(|list| list.items())
            .into_iter()
            .flatten()
    }
}

impl Impl {
    pub fn self_ty(&self) -> Option<Type> {
        support::child(&self.0)
    }

    pub fn item_list(&self) -> Option<ItemList> {
        support::child(&self.0)
    }

    pub fn items(&self) -> std::iter::Flatten<std::option::IntoIter<AstChildren<Item>>> {
        self.item_list()
            .map(|list| list.items())
            .into_iter()
            .flatten()
    }
}

impl ItemList {
    pub fn items(&self) -> AstChildren<Item> {
        support::children(&self.0)
    }
}

impl AssetImport {
    /// The `"path"` string token, quotes included.
    pub fn path_token(&self) -> Option<SyntaxToken> {
        support::token(&self.0, RepresentKind::STRING)
    }

    /// The unescaped path; `None` if the token is missing or malformed.
    pub fn path_value(&self) -> Option<String> {
        unescape_string(self.path_token()?.text()).ok()
    }

    pub fn ty(&self) -> Option<Type> {
        support::child(&self.0)
    }
}

ast_node!(GenericParamList, GENERIC_PARAM_LIST);
ast_node!(GenericParam, GENERIC_PARAM);
ast_node!(
    /// `<T, U>` in a type, or the turbofish `::<T>` in an expression.
    GenericArgList, GENERIC_ARG_LIST
);

impl GenericParamList {
    pub fn params(&self) -> AstChildren<GenericParam> {
        support::children(&self.0)
    }
}

impl GenericArgList {
    pub fn types(&self) -> AstChildren<Type> {
        support::children(&self.0)
    }
}

// ---- names, paths, types ------------------------------------------------------------------

ast_node!(
    /// A defining name.
    Name, NAME
);
ast_node!(
    /// A referring name.
    NameRef, NAME_REF
);

impl Name {
    pub fn ident_token(&self) -> Option<SyntaxToken> {
        support::token(&self.0, RepresentKind::IDENT)
    }

    pub fn text(&self) -> String {
        token_text(self.ident_token())
    }
}

impl NameRef {
    pub fn ident_token(&self) -> Option<SyntaxToken> {
        support::token(&self.0, RepresentKind::IDENT)
    }

    pub fn text(&self) -> String {
        token_text(self.ident_token())
    }
}

ast_node!(
    /// `a::b::<T>::c`, segments flat.
    Path, PATH
);
ast_node!(PathSegment, PATH_SEGMENT);

/// What a path segment names.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PathSegmentKind {
    Name(NameRef),
    /// `self`
    SelfValue,
    /// `Self`
    SelfType,
}

impl Path {
    pub fn segments(&self) -> AstChildren<PathSegment> {
        support::children(&self.0)
    }

    /// The only segment, if the path has exactly one.
    pub fn single_segment(&self) -> Option<PathSegment> {
        let mut segments = self.segments();
        let first = segments.next()?;
        segments.next().is_none().then_some(first)
    }
}

impl PathSegment {
    pub fn name_ref(&self) -> Option<NameRef> {
        support::child(&self.0)
    }

    pub fn self_token(&self) -> Option<SyntaxToken> {
        support::token(&self.0, RepresentKind::SELF_KW)
    }

    pub fn self_type_token(&self) -> Option<SyntaxToken> {
        support::token(&self.0, RepresentKind::SELF_TYPE_KW)
    }

    pub fn generic_arg_list(&self) -> Option<GenericArgList> {
        support::child(&self.0)
    }

    pub fn kind(&self) -> Option<PathSegmentKind> {
        if let Some(name) = self.name_ref() {
            return Some(PathSegmentKind::Name(name));
        }
        if self.self_token().is_some() {
            return Some(PathSegmentKind::SelfValue);
        }
        self.self_type_token().map(|_| PathSegmentKind::SelfType)
    }

    /// The segment's text: the name, `self` or `Self`.
    pub fn text(&self) -> Option<String> {
        Some(match self.kind()? {
            PathSegmentKind::Name(name) => name.text(),
            PathSegmentKind::SelfValue => "self".to_owned(),
            PathSegmentKind::SelfType => "Self".to_owned(),
        })
    }
}

ast_enum!(
    Type {
        PathType(PATH_TYPE),
        UnitType(UNIT_TYPE),
    }
);

ast_node!(PathType, PATH_TYPE);
ast_node!(
    /// `()`
    UnitType, UNIT_TYPE
);

impl PathType {
    pub fn path(&self) -> Option<Path> {
        support::child(&self.0)
    }
}

// ---- statements ---------------------------------------------------------------------------

ast_enum!(
    Stmt {
        LetStmt(LET_STMT),
        ExprStmt(EXPR_STMT),
    }
);

ast_node!(BlockExpr, BLOCK_EXPR);
ast_node!(LetStmt, LET_STMT);
ast_node!(
    /// An expression statement; the `;` is optional after `match`/block expressions.
    ExprStmt, EXPR_STMT
);

impl BlockExpr {
    pub fn stmts(&self) -> AstChildren<Stmt> {
        support::children(&self.0)
    }

    /// The trailing expression without `;`, the block's value.
    pub fn tail_expr(&self) -> Option<Expr> {
        support::children::<Expr>(&self.0).last()
    }
}

impl LetStmt {
    pub fn pat(&self) -> Option<Pat> {
        support::child(&self.0)
    }

    pub fn ty(&self) -> Option<Type> {
        support::child(&self.0)
    }

    pub fn initializer(&self) -> Option<Expr> {
        support::child(&self.0)
    }

    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.0, RepresentKind::SEMICOLON)
    }
}

impl ExprStmt {
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.0)
    }

    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        support::token(&self.0, RepresentKind::SEMICOLON)
    }
}

// ---- expressions --------------------------------------------------------------------------

ast_enum!(
    Expr {
        Literal(LITERAL),
        PathExpr(PATH_EXPR),
        RecordExpr(RECORD_EXPR),
        ParenExpr(PAREN_EXPR),
        UnitExpr(UNIT_EXPR),
        BlockExpr(BLOCK_EXPR),
        MatchExpr(MATCH_EXPR),
        CallExpr(CALL_EXPR),
        MethodCallExpr(METHOD_CALL_EXPR),
        FieldExpr(FIELD_EXPR),
        CastExpr(CAST_EXPR),
        BinExpr(BIN_EXPR),
        PrefixExpr(PREFIX_EXPR),
    }
);

impl Expr {
    /// `match` and block expressions, which end a statement without `;`.
    pub fn is_block_like(&self) -> bool {
        matches!(self, Expr::BlockExpr(_) | Expr::MatchExpr(_))
    }
}

ast_node!(Literal, LITERAL);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LiteralKind {
    Int,
    Decimal,
    Str,
    Bool(bool),
}

impl Literal {
    pub fn token(&self) -> Option<SyntaxToken> {
        tokens(&self.0).find(|t| !t.kind().is_trivia())
    }

    pub fn kind(&self) -> Option<LiteralKind> {
        Some(match self.token()?.kind() {
            RepresentKind::INT_NUMBER => LiteralKind::Int,
            RepresentKind::DECIMAL_NUMBER => LiteralKind::Decimal,
            RepresentKind::STRING => LiteralKind::Str,
            RepresentKind::TRUE_KW => LiteralKind::Bool(true),
            RepresentKind::FALSE_KW => LiteralKind::Bool(false),
            _ => return None,
        })
    }

    /// The value of an integer literal; `None` if it is not one or does not fit `u128`.
    pub fn int_value(&self) -> Option<u128> {
        let token = self.token()?;
        if token.kind() != RepresentKind::INT_NUMBER {
            return None;
        }
        parse_digits(token.text())
    }

    /// A decimal literal as the exact fraction `numerator / denominator` (`0.75` → `(75, 100)`).
    pub fn decimal_value(&self) -> Option<(u128, u128)> {
        let token = self.token()?;
        if token.kind() != RepresentKind::DECIMAL_NUMBER {
            return None;
        }
        let (int, frac) = token.text().split_once('.')?;
        let frac_digits: String = frac.chars().filter(|c| *c != '_').collect();
        let numerator = parse_digits(&format!("{int}{frac_digits}"))?;
        let exponent = u32::try_from(frac_digits.len()).ok()?;
        let denominator = 10u128.checked_pow(exponent)?;
        Some((numerator, denominator))
    }

    /// The unescaped value of a string literal.
    pub fn string_value(&self) -> Option<String> {
        let token = self.token()?;
        if token.kind() != RepresentKind::STRING {
            return None;
        }
        unescape_string(token.text()).ok()
    }
}

fn parse_digits(text: &str) -> Option<u128> {
    let digits: String = text.chars().filter(|c| *c != '_').collect();
    digits.parse().ok()
}

/// Unescapes a string literal token (quotes included). Supported escapes are
/// `\n \r \t \\ \" \0 \u{X…}`. On failure returns the first error with its range relative to
/// the start of `text`.
pub fn unescape_string(text: &str) -> Result<String, (TextRange, String)> {
    let whole = TextRange::up_to(text_size(text.len()));
    let Some(body) = text.strip_prefix('"') else {
        return Err((whole, "expected a string literal".to_owned()));
    };
    // Offsets below are relative to `body`; `rel` shifts them past the opening quote.
    let rel = |start: usize, end: usize| {
        TextRange::new(
            text_size(start.saturating_add(1)),
            text_size(end.saturating_add(1)),
        )
    };
    let mut out = String::with_capacity(body.len());
    let mut chars = body.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Ok(out),
            '\\' => {
                let Some((j, escape)) = chars.next() else {
                    break;
                };
                let after = j.saturating_add(escape.len_utf8());
                let simple = match escape {
                    'n' => Some('\n'),
                    'r' => Some('\r'),
                    't' => Some('\t'),
                    '\\' => Some('\\'),
                    '"' => Some('"'),
                    '0' => Some('\0'),
                    _ => None,
                };
                if let Some(ch) = simple {
                    out.push(ch);
                    continue;
                }
                if escape != 'u' {
                    return Err((
                        rel(i, after),
                        format!("unknown character escape `\\{escape}`"),
                    ));
                }
                let mut end = after;
                if chars.next_if(|&(_, c)| c == '{').is_none() {
                    return Err((rel(i, end), "expected `{` after `\\u`".to_owned()));
                }
                end = end.saturating_add(1);
                let mut value = 0u32;
                let mut digits = 0usize;
                let mut closed = false;
                while let Some((k, c)) = chars.next_if(|&(_, c)| c != '"') {
                    end = k.saturating_add(c.len_utf8());
                    if c == '}' {
                        closed = true;
                        break;
                    }
                    if c == '_' {
                        continue;
                    }
                    let Some(digit) = c.to_digit(16) else {
                        return Err((
                            rel(i, end),
                            "invalid character in unicode escape".to_owned(),
                        ));
                    };
                    digits = digits.saturating_add(1);
                    value = value.saturating_mul(16).saturating_add(digit);
                }
                if !closed {
                    return Err((rel(i, end), "unterminated unicode escape".to_owned()));
                }
                if digits == 0 || digits > 6 {
                    return Err((
                        rel(i, end),
                        "unicode escape must have 1 to 6 hex digits".to_owned(),
                    ));
                }
                let Some(ch) = char::from_u32(value) else {
                    return Err((rel(i, end), "invalid unicode character escape".to_owned()));
                };
                out.push(ch);
            }
            _ => out.push(c),
        }
    }
    Err((whole, "unterminated string literal".to_owned()))
}

ast_node!(PathExpr, PATH_EXPR);

impl PathExpr {
    pub fn path(&self) -> Option<Path> {
        support::child(&self.0)
    }
}

ast_node!(
    /// `Path { field, field: expr, ..base }`
    RecordExpr, RECORD_EXPR
);
ast_node!(RecordExprFieldList, RECORD_EXPR_FIELD_LIST);
ast_node!(
    /// `name: expr`, or the shorthand `name` (then `expr()` is `None`).
    RecordExprField, RECORD_EXPR_FIELD
);

impl RecordExpr {
    pub fn path(&self) -> Option<Path> {
        support::child(&self.0)
    }

    pub fn record_expr_field_list(&self) -> Option<RecordExprFieldList> {
        support::child(&self.0)
    }

    pub fn fields(
        &self,
    ) -> std::iter::Flatten<std::option::IntoIter<AstChildren<RecordExprField>>> {
        self.record_expr_field_list()
            .map(|list| list.fields())
            .into_iter()
            .flatten()
    }

    /// The `..base` expression.
    pub fn spread(&self) -> Option<Expr> {
        self.record_expr_field_list()?.spread()
    }
}

impl RecordExprFieldList {
    pub fn fields(&self) -> AstChildren<RecordExprField> {
        support::children(&self.0)
    }

    pub fn dotdot_token(&self) -> Option<SyntaxToken> {
        support::token(&self.0, RepresentKind::DOT2)
    }

    pub fn spread(&self) -> Option<Expr> {
        support::child(&self.0)
    }
}

impl RecordExprField {
    pub fn name_ref(&self) -> Option<NameRef> {
        support::child(&self.0)
    }

    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.0)
    }
}

ast_node!(ParenExpr, PAREN_EXPR);
ast_node!(
    /// `()`
    UnitExpr, UNIT_EXPR
);

impl ParenExpr {
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.0)
    }
}

ast_node!(MatchExpr, MATCH_EXPR);
ast_node!(MatchArmList, MATCH_ARM_LIST);
ast_node!(MatchArm, MATCH_ARM);

impl MatchExpr {
    pub fn scrutinee(&self) -> Option<Expr> {
        support::child(&self.0)
    }

    pub fn match_arm_list(&self) -> Option<MatchArmList> {
        support::child(&self.0)
    }

    pub fn arms(&self) -> std::iter::Flatten<std::option::IntoIter<AstChildren<MatchArm>>> {
        self.match_arm_list()
            .map(|list| list.arms())
            .into_iter()
            .flatten()
    }
}

impl MatchArmList {
    pub fn arms(&self) -> AstChildren<MatchArm> {
        support::children(&self.0)
    }
}

impl MatchArm {
    pub fn pat(&self) -> Option<Pat> {
        support::child(&self.0)
    }

    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.0)
    }
}

ast_node!(CallExpr, CALL_EXPR);
ast_node!(ArgList, ARG_LIST);
ast_node!(
    /// `receiver.name::<T>(args)`
    MethodCallExpr, METHOD_CALL_EXPR
);
ast_node!(
    /// `receiver.name`
    FieldExpr, FIELD_EXPR
);

impl CallExpr {
    pub fn callee(&self) -> Option<Expr> {
        support::child(&self.0)
    }

    pub fn arg_list(&self) -> Option<ArgList> {
        support::child(&self.0)
    }
}

impl ArgList {
    pub fn args(&self) -> AstChildren<Expr> {
        support::children(&self.0)
    }
}

impl MethodCallExpr {
    pub fn receiver(&self) -> Option<Expr> {
        support::child(&self.0)
    }

    pub fn name_ref(&self) -> Option<NameRef> {
        support::child(&self.0)
    }

    pub fn generic_arg_list(&self) -> Option<GenericArgList> {
        support::child(&self.0)
    }

    pub fn arg_list(&self) -> Option<ArgList> {
        support::child(&self.0)
    }
}

impl FieldExpr {
    pub fn receiver(&self) -> Option<Expr> {
        support::child(&self.0)
    }

    pub fn name_ref(&self) -> Option<NameRef> {
        support::child(&self.0)
    }
}

ast_node!(
    /// `expr as Type`
    CastExpr, CAST_EXPR
);

impl CastExpr {
    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.0)
    }

    pub fn ty(&self) -> Option<Type> {
        support::child(&self.0)
    }
}

ast_node!(BinExpr, BIN_EXPR);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl BinOp {
    pub fn from_kind(kind: RepresentKind) -> Option<BinOp> {
        Some(match kind {
            RepresentKind::PLUS => BinOp::Add,
            RepresentKind::MINUS => BinOp::Sub,
            RepresentKind::STAR => BinOp::Mul,
            RepresentKind::SLASH => BinOp::Div,
            RepresentKind::PERCENT => BinOp::Rem,
            RepresentKind::EQ2 => BinOp::Eq,
            RepresentKind::NEQ => BinOp::Ne,
            RepresentKind::L_ANGLE => BinOp::Lt,
            RepresentKind::LTEQ => BinOp::Le,
            RepresentKind::R_ANGLE => BinOp::Gt,
            RepresentKind::GTEQ => BinOp::Ge,
            RepresentKind::AMP2 => BinOp::And,
            RepresentKind::PIPE2 => BinOp::Or,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "&&",
            BinOp::Or => "||",
        }
    }

    /// `+ - * / %`
    pub fn is_arithmetic(self) -> bool {
        matches!(
            self,
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem
        )
    }

    /// `< <= > >=`
    pub fn is_ordering(self) -> bool {
        matches!(self, BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge)
    }

    /// `== !=`
    pub fn is_equality(self) -> bool {
        matches!(self, BinOp::Eq | BinOp::Ne)
    }

    /// `&& ||`
    pub fn is_logical(self) -> bool {
        matches!(self, BinOp::And | BinOp::Or)
    }
}

impl BinExpr {
    pub fn op_token(&self) -> Option<SyntaxToken> {
        tokens(&self.0).find(|t| BinOp::from_kind(t.kind()).is_some())
    }

    pub fn op_kind(&self) -> Option<BinOp> {
        BinOp::from_kind(self.op_token()?.kind())
    }

    pub fn lhs(&self) -> Option<Expr> {
        let op = self.op_token()?;
        self.0
            .children()
            .take_while(|node| node.text_range().end() <= op.text_range().start())
            .filter_map(Expr::cast)
            .last()
    }

    pub fn rhs(&self) -> Option<Expr> {
        let op = self.op_token()?;
        self.0
            .children()
            .filter(|node| node.text_range().start() >= op.text_range().end())
            .find_map(Expr::cast)
    }
}

ast_node!(
    /// `-expr` or `!expr`
    PrefixExpr, PREFIX_EXPR
);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PrefixOp {
    /// `-`
    Neg,
    /// `!`
    Not,
}

impl PrefixExpr {
    pub fn op_token(&self) -> Option<SyntaxToken> {
        tokens(&self.0).find(|t| matches!(t.kind(), RepresentKind::MINUS | RepresentKind::BANG))
    }

    pub fn op_kind(&self) -> Option<PrefixOp> {
        Some(match self.op_token()?.kind() {
            RepresentKind::MINUS => PrefixOp::Neg,
            _ => PrefixOp::Not,
        })
    }

    pub fn expr(&self) -> Option<Expr> {
        support::child(&self.0)
    }
}

// ---- patterns -----------------------------------------------------------------------------

ast_enum!(
    Pat {
        OrPat(OR_PAT),
        WildcardPat(WILDCARD_PAT),
        LiteralPat(LITERAL_PAT),
        RangePat(RANGE_PAT),
        TupleStructPat(TUPLE_STRUCT_PAT),
        RecordPat(RECORD_PAT),
        PathPat(PATH_PAT),
        IdentPat(IDENT_PAT),
    }
);

ast_node!(
    /// `a | b | c`
    OrPat, OR_PAT
);
ast_node!(
    /// `_`
    WildcardPat, WILDCARD_PAT
);
ast_node!(
    /// `-1`, `3`, `true`
    LiteralPat, LITERAL_PAT
);
ast_node!(
    /// `lo..=hi`
    RangePat, RANGE_PAT
);
ast_node!(
    /// `Path(p, q)`
    TupleStructPat, TUPLE_STRUCT_PAT
);
ast_node!(
    /// `Path { a: p, b, .. }`
    RecordPat, RECORD_PAT
);
ast_node!(RecordPatFieldList, RECORD_PAT_FIELD_LIST);
ast_node!(
    /// `name: pat`, or the shorthand `name`, which binds `name` (an `IdentPat` child).
    RecordPatField, RECORD_PAT_FIELD
);
ast_node!(
    /// `..` in a record pattern.
    RestPat, REST_PAT
);
ast_node!(
    /// A path pattern such as `Enum::Variant`.
    PathPat, PATH_PAT
);
ast_node!(
    /// A single-name pattern: a binding, or a unit variant/const in scope.
    IdentPat, IDENT_PAT
);

impl OrPat {
    pub fn pats(&self) -> AstChildren<Pat> {
        support::children(&self.0)
    }
}

impl LiteralPat {
    pub fn minus_token(&self) -> Option<SyntaxToken> {
        support::token(&self.0, RepresentKind::MINUS)
    }

    pub fn negative(&self) -> bool {
        self.minus_token().is_some()
    }

    pub fn literal(&self) -> Option<Literal> {
        support::child(&self.0)
    }
}

impl RangePat {
    pub fn op_token(&self) -> Option<SyntaxToken> {
        tokens(&self.0).find(|t| matches!(t.kind(), RepresentKind::DOT2EQ | RepresentKind::DOT2))
    }

    pub fn start(&self) -> Option<LiteralPat> {
        let op = self.op_token()?;
        self.0
            .children()
            .take_while(|node| node.text_range().end() <= op.text_range().start())
            .find_map(LiteralPat::cast)
    }

    pub fn end(&self) -> Option<LiteralPat> {
        let op = self.op_token()?;
        self.0
            .children()
            .filter(|node| node.text_range().start() >= op.text_range().end())
            .find_map(LiteralPat::cast)
    }
}

impl TupleStructPat {
    pub fn path(&self) -> Option<Path> {
        support::child(&self.0)
    }

    pub fn fields(&self) -> AstChildren<Pat> {
        support::children(&self.0)
    }
}

impl RecordPat {
    pub fn path(&self) -> Option<Path> {
        support::child(&self.0)
    }

    pub fn record_pat_field_list(&self) -> Option<RecordPatFieldList> {
        support::child(&self.0)
    }

    pub fn fields(&self) -> std::iter::Flatten<std::option::IntoIter<AstChildren<RecordPatField>>> {
        self.record_pat_field_list()
            .map(|list| list.fields())
            .into_iter()
            .flatten()
    }

    pub fn rest(&self) -> Option<RestPat> {
        self.record_pat_field_list()?.rest()
    }
}

impl RecordPatFieldList {
    pub fn fields(&self) -> AstChildren<RecordPatField> {
        support::children(&self.0)
    }

    pub fn rest(&self) -> Option<RestPat> {
        support::child(&self.0)
    }
}

impl RecordPatField {
    /// The explicit field name of `name: pat`; `None` for the shorthand.
    pub fn name_ref(&self) -> Option<NameRef> {
        support::child(&self.0)
    }

    /// The sub-pattern; for the shorthand `name` this is the binding `IdentPat`.
    pub fn pat(&self) -> Option<Pat> {
        support::child(&self.0)
    }

    /// The field this entry matches, in both forms.
    pub fn field_name(&self) -> Option<String> {
        if let Some(name) = self.name_ref() {
            return Some(name.text());
        }
        match self.pat()? {
            Pat::IdentPat(pat) => pat.name().map(|name| name.text()),
            _ => None,
        }
    }
}

impl PathPat {
    pub fn path(&self) -> Option<Path> {
        support::child(&self.0)
    }
}

impl IdentPat {
    pub fn name(&self) -> Option<Name> {
        support::child(&self.0)
    }
}
