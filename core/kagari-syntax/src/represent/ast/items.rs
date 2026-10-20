//! Roots, items, attributes and generic parameter lists.

use rowan::ast::{AstChildren, support};

use super::{
    BlockExpr, Expr, HasAttrs, HasGenericParams, HasName, NameRef, Pat, Stmt, Type, unescape_string,
};
use crate::represent::cst::{RepresentKind, SyntaxToken};

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
