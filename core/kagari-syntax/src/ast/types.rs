//! Types.

use rowan::ast::AstChildren;

use super::{Expr, ParamList, Path, RetType, child, child_after, children, first_token, token};
use crate::cst::{SyntaxKind, SyntaxToken};

ast_node!(PathType, PATH_TYPE);
ast_node!(TupleType, TUPLE_TYPE);
ast_node!(ParenType, PAREN_TYPE);
ast_node!(ArrayType, ARRAY_TYPE);
ast_node!(RefType, REF_TYPE);
ast_node!(InferType, INFER_TYPE);
ast_node!(FnPtrType, FN_PTR_TYPE);
ast_node!(ClosureType, CLOSURE_TYPE);

ast_enum!(Type {
    PathType(PATH_TYPE),
    TupleType(TUPLE_TYPE),
    ParenType(PAREN_TYPE),
    ArrayType(ARRAY_TYPE),
    RefType(REF_TYPE),
    InferType(INFER_TYPE),
    FnPtrType(FN_PTR_TYPE),
    ClosureType(CLOSURE_TYPE),
});

/// The kind of a function-pointer type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FnPtrKind {
    Plain,
    Const,
    Pushdown,
    Scene,
}

/// The kind of a closure type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClosureKind {
    Plain,
    Const,
    Pushdown,
}

impl PathType {
    pub fn path(&self) -> Option<Path> {
        child(&self.0)
    }
}

impl TupleType {
    pub fn fields(&self) -> AstChildren<Type> {
        children(&self.0)
    }
}

impl ParenType {
    pub fn ty(&self) -> Option<Type> {
        child(&self.0)
    }
}

impl ArrayType {
    pub fn ty(&self) -> Option<Type> {
        child(&self.0)
    }

    /// The length expression after `;`.
    pub fn len(&self) -> Option<Expr> {
        child_after(&self.0, SyntaxKind::SEMICOLON)
    }
}

impl RefType {
    pub fn amp_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::AMP)
    }

    pub fn mut_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::MUT_KW)
    }

    pub fn ty(&self) -> Option<Type> {
        child(&self.0)
    }
}

impl InferType {
    pub fn underscore_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::UNDERSCORE)
    }
}

impl FnPtrType {
    pub fn kind(&self) -> FnPtrKind {
        match first_token(&self.0).map(|t| t.kind()) {
            Some(SyntaxKind::CONST_KW) => FnPtrKind::Const,
            Some(SyntaxKind::PUSHDOWN_KW) => FnPtrKind::Pushdown,
            Some(SyntaxKind::SCENE_KW) => FnPtrKind::Scene,
            _ => FnPtrKind::Plain,
        }
    }

    /// Parameters are `Param`s holding only a type.
    pub fn param_list(&self) -> Option<ParamList> {
        child(&self.0)
    }

    pub fn ret_type(&self) -> Option<RetType> {
        child(&self.0)
    }
}

impl ClosureType {
    pub fn kind(&self) -> ClosureKind {
        match first_token(&self.0).map(|t| t.kind()) {
            Some(SyntaxKind::CONST_KW) => ClosureKind::Const,
            Some(SyntaxKind::PUSHDOWN_KW) => ClosureKind::Pushdown,
            _ => ClosureKind::Plain,
        }
    }

    /// Parameters are `Param`s holding only a type.
    pub fn param_list(&self) -> Option<ParamList> {
        child(&self.0)
    }

    pub fn ret_type(&self) -> Option<RetType> {
        child(&self.0)
    }
}
