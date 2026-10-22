//! Items, attributes, generics and use trees.

use rowan::ast::AstChildren;

use super::{
    BlockExpr, Expr, Path, Type, child, child_after, child_before, children, first_token, token,
};
use crate::cst::{SyntaxKind, SyntaxNode, SyntaxToken};

ast_node!(SourceFile, SOURCE_FILE);
ast_node!(Attr, ATTR);
ast_node!(TokenTree, TOKEN_TREE);
ast_node!(Visibility, VISIBILITY);
ast_node!(Fn, FN);
ast_node!(ParamList, PARAM_LIST);
ast_node!(SelfParam, SELF_PARAM);
ast_node!(Param, PARAM);
ast_node!(RetType, RET_TYPE);
ast_node!(Struct, STRUCT);
ast_node!(RecordFieldList, RECORD_FIELD_LIST);
ast_node!(RecordField, RECORD_FIELD);
ast_node!(TupleFieldList, TUPLE_FIELD_LIST);
ast_node!(TupleField, TUPLE_FIELD);
ast_node!(Enum, ENUM);
ast_node!(VariantList, VARIANT_LIST);
ast_node!(Variant, VARIANT);
ast_node!(TypeAlias, TYPE_ALIAS);
ast_node!(Const, CONST);
ast_node!(Static, STATIC);
ast_node!(Mod, MOD);
ast_node!(ItemList, ITEM_LIST);
ast_node!(Use, USE);
ast_node!(UseTree, USE_TREE);
ast_node!(UseTreeList, USE_TREE_LIST);
ast_node!(Rename, RENAME);
ast_node!(Impl, IMPL);
ast_node!(Trait, TRAIT);
ast_node!(AssocItemList, ASSOC_ITEM_LIST);
ast_node!(GenericParamList, GENERIC_PARAM_LIST);
ast_node!(TypeParam, TYPE_PARAM);
ast_node!(ConstParam, CONST_PARAM);
ast_node!(TypeBoundList, TYPE_BOUND_LIST);
ast_node!(TypeBound, TYPE_BOUND);

ast_enum!(
    /// Any item.
    Item {
        Fn(FN),
        Struct(STRUCT),
        Enum(ENUM),
        TypeAlias(TYPE_ALIAS),
        Const(CONST),
        Static(STATIC),
        Mod(MOD),
        Use(USE),
        Impl(IMPL),
        Trait(TRAIT),
    }
);

ast_enum!(
    /// An item of an `impl` or `trait`.
    AssocItem {
        Fn(FN),
        Const(CONST),
        TypeAlias(TYPE_ALIAS),
    }
);

ast_enum!(
    /// The fields of a struct or variant.
    FieldList {
        RecordFieldList(RECORD_FIELD_LIST),
        TupleFieldList(TUPLE_FIELD_LIST),
    }
);

ast_enum!(GenericParam {
    TypeParam(TYPE_PARAM),
    ConstParam(CONST_PARAM),
});

impl SourceFile {
    pub(crate) fn from_root(node: SyntaxNode) -> SourceFile {
        SourceFile(node)
    }

    pub fn items(&self) -> AstChildren<Item> {
        children(&self.0)
    }
}

impl Attr {
    pub fn path(&self) -> Option<Path> {
        child(&self.0)
    }

    pub fn token_tree(&self) -> Option<TokenTree> {
        child(&self.0)
    }

    /// The name of a single-segment attribute path (`#[scene]` → `scene`).
    pub fn name_text(&self) -> Option<String> {
        let path = self.path()?;
        if path.qualifier().is_some() {
            return None;
        }
        Some(path.segment()?.name_ref()?.text())
    }
}

impl Visibility {
    pub fn pub_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::PUB_KW)
    }
}

/// The kind of a function: `fn`, `const fn` or `pushdown fn`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FnKind {
    Plain,
    Const,
    Pushdown,
}

impl Fn {
    pub fn fn_kind(&self) -> FnKind {
        if self.const_token().is_some() {
            FnKind::Const
        } else if self.pushdown_token().is_some() {
            FnKind::Pushdown
        } else {
            FnKind::Plain
        }
    }

    pub fn const_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::CONST_KW)
    }

    pub fn pushdown_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::PUSHDOWN_KW)
    }

    pub fn fn_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::FN_KW)
    }

    pub fn param_list(&self) -> Option<ParamList> {
        child(&self.0)
    }

    pub fn ret_type(&self) -> Option<RetType> {
        child(&self.0)
    }

    pub fn body(&self) -> Option<BlockExpr> {
        child(&self.0)
    }

    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::SEMICOLON)
    }
}

impl ParamList {
    pub fn self_param(&self) -> Option<SelfParam> {
        child(&self.0)
    }

    pub fn params(&self) -> AstChildren<Param> {
        children(&self.0)
    }
}

impl SelfParam {
    pub fn amp_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::AMP)
    }

    pub fn mut_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::MUT_KW)
    }

    pub fn self_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::SELF_KW)
    }
}

impl Param {
    /// The pattern of a function or closure parameter; `None` in function-pointer and
    /// closure types.
    pub fn pat(&self) -> Option<super::Pat> {
        child(&self.0)
    }

    pub fn ty(&self) -> Option<Type> {
        child(&self.0)
    }
}

impl RetType {
    pub fn ty(&self) -> Option<Type> {
        child(&self.0)
    }
}

impl Struct {
    pub fn field_list(&self) -> Option<FieldList> {
        child(&self.0)
    }

    pub fn record_field_list(&self) -> Option<RecordFieldList> {
        child(&self.0)
    }

    pub fn tuple_field_list(&self) -> Option<TupleFieldList> {
        child(&self.0)
    }

    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::SEMICOLON)
    }
}

impl RecordFieldList {
    pub fn fields(&self) -> AstChildren<RecordField> {
        children(&self.0)
    }
}

impl RecordField {
    pub fn ty(&self) -> Option<Type> {
        child(&self.0)
    }
}

impl TupleFieldList {
    pub fn fields(&self) -> AstChildren<TupleField> {
        children(&self.0)
    }
}

impl TupleField {
    pub fn ty(&self) -> Option<Type> {
        child(&self.0)
    }
}

impl Enum {
    pub fn variant_list(&self) -> Option<VariantList> {
        child(&self.0)
    }
}

impl VariantList {
    pub fn variants(&self) -> AstChildren<Variant> {
        children(&self.0)
    }
}

impl Variant {
    pub fn field_list(&self) -> Option<FieldList> {
        child(&self.0)
    }

    pub fn record_field_list(&self) -> Option<RecordFieldList> {
        child(&self.0)
    }

    pub fn tuple_field_list(&self) -> Option<TupleFieldList> {
        child(&self.0)
    }
}

impl TypeAlias {
    pub fn type_bound_list(&self) -> Option<TypeBoundList> {
        child(&self.0)
    }

    /// The aliased type after `=`.
    pub fn ty(&self) -> Option<Type> {
        child_after(&self.0, SyntaxKind::EQ)
    }

    pub fn eq_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::EQ)
    }
}

impl Const {
    pub fn ty(&self) -> Option<Type> {
        child_before(&self.0, SyntaxKind::EQ)
    }

    /// The value after `=`.
    pub fn body(&self) -> Option<Expr> {
        child_after(&self.0, SyntaxKind::EQ)
    }

    pub fn eq_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::EQ)
    }
}

impl Static {
    pub fn mut_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::MUT_KW)
    }

    pub fn ty(&self) -> Option<Type> {
        child_before(&self.0, SyntaxKind::EQ)
    }

    /// The initializer after `=`.
    pub fn body(&self) -> Option<Expr> {
        child_after(&self.0, SyntaxKind::EQ)
    }
}

impl Mod {
    /// The `{ … }` of an inline module; `None` for `mod name;`.
    pub fn item_list(&self) -> Option<ItemList> {
        child(&self.0)
    }

    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::SEMICOLON)
    }
}

impl ItemList {
    pub fn items(&self) -> AstChildren<Item> {
        children(&self.0)
    }
}

impl Use {
    pub fn use_tree(&self) -> Option<UseTree> {
        child(&self.0)
    }
}

impl UseTree {
    pub fn path(&self) -> Option<Path> {
        child(&self.0)
    }

    pub fn rename(&self) -> Option<Rename> {
        child(&self.0)
    }

    pub fn use_tree_list(&self) -> Option<UseTreeList> {
        child(&self.0)
    }

    pub fn star_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::STAR)
    }

    pub fn coloncolon_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::COLON2)
    }
}

impl UseTreeList {
    pub fn use_trees(&self) -> AstChildren<UseTree> {
        children(&self.0)
    }
}

impl Rename {
    pub fn name(&self) -> Option<super::Name> {
        child(&self.0)
    }

    /// `as _`
    pub fn underscore_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::UNDERSCORE)
    }
}

impl Impl {
    /// The trait of `impl Trait for Type`.
    pub fn trait_type(&self) -> Option<Type> {
        self.for_token()?;
        child_before(&self.0, SyntaxKind::FOR_KW)
    }

    /// The implementing type: after `for`, or the only type of an inherent impl.
    pub fn self_type(&self) -> Option<Type> {
        if self.for_token().is_some() {
            child_after(&self.0, SyntaxKind::FOR_KW)
        } else {
            child(&self.0)
        }
    }

    pub fn for_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::FOR_KW)
    }

    pub fn assoc_item_list(&self) -> Option<AssocItemList> {
        child(&self.0)
    }
}

impl Trait {
    pub fn type_bound_list(&self) -> Option<TypeBoundList> {
        child(&self.0)
    }

    pub fn assoc_item_list(&self) -> Option<AssocItemList> {
        child(&self.0)
    }
}

impl AssocItemList {
    pub fn assoc_items(&self) -> AstChildren<AssocItem> {
        children(&self.0)
    }
}

impl GenericParamList {
    pub fn generic_params(&self) -> AstChildren<GenericParam> {
        children(&self.0)
    }
}

impl TypeParam {
    pub fn type_bound_list(&self) -> Option<TypeBoundList> {
        child(&self.0)
    }
}

impl ConstParam {
    pub fn ty(&self) -> Option<Type> {
        child(&self.0)
    }
}

impl TypeBoundList {
    pub fn bounds(&self) -> AstChildren<TypeBound> {
        children(&self.0)
    }
}

impl TypeBound {
    pub fn ty(&self) -> Option<Type> {
        child(&self.0)
    }
}

impl TokenTree {
    /// The first token (the opening delimiter).
    pub fn l_delim_token(&self) -> Option<SyntaxToken> {
        first_token(&self.0)
    }
}
