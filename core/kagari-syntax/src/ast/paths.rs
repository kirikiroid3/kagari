//! Names, paths and generic arguments.

use rowan::ast::AstChildren;

use super::{Expr, Type, child, children, token};
use crate::cst::{SyntaxKind, SyntaxToken};

ast_node!(Name, NAME);
ast_node!(NameRef, NAME_REF);
ast_node!(Path, PATH);
ast_node!(PathSegment, PATH_SEGMENT);
ast_node!(GenericArgList, GENERIC_ARG_LIST);
ast_node!(TypeArg, TYPE_ARG);
ast_node!(ConstArg, CONST_ARG);

ast_enum!(GenericArg {
    TypeArg(TYPE_ARG),
    ConstArg(CONST_ARG),
});

impl Name {
    pub fn ident_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::IDENT)
    }

    /// The identifier, without trivia.
    pub fn text(&self) -> String {
        self.ident_token()
            .map(|t| t.text().to_owned())
            .unwrap_or_default()
    }
}

impl NameRef {
    pub fn ident_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::IDENT)
    }

    /// The identifier, without trivia.
    pub fn text(&self) -> String {
        self.ident_token()
            .map(|t| t.text().to_owned())
            .unwrap_or_default()
    }
}

/// What a path segment names.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PathSegmentKind {
    Name(NameRef),
    /// `self`
    SelfKw,
    /// `super`
    SuperKw,
    /// `crate`
    CrateKw,
    /// `Self`
    SelfTypeKw,
}

impl Path {
    /// The path before the last `::` (`a::b` of `a::b::c`).
    pub fn qualifier(&self) -> Option<Path> {
        child(&self.0)
    }

    /// The last segment.
    pub fn segment(&self) -> Option<PathSegment> {
        child(&self.0)
    }

    pub fn coloncolon_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::COLON2)
    }

    /// All segments, first to last.
    pub fn segments(&self) -> impl Iterator<Item = PathSegment> + use<> {
        let mut paths = vec![self.clone()];
        while let Some(qualifier) = paths.last().and_then(Path::qualifier) {
            paths.push(qualifier);
        }
        paths.into_iter().rev().filter_map(|path| path.segment())
    }

    /// The first segment.
    pub fn first_segment(&self) -> Option<PathSegment> {
        self.segments().next()
    }
}

impl PathSegment {
    pub fn kind(&self) -> Option<PathSegmentKind> {
        if let Some(name_ref) = self.name_ref() {
            return Some(PathSegmentKind::Name(name_ref));
        }
        self.0
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find_map(|t| match t.kind() {
                SyntaxKind::SELF_KW => Some(PathSegmentKind::SelfKw),
                SyntaxKind::SUPER_KW => Some(PathSegmentKind::SuperKw),
                SyntaxKind::CRATE_KW => Some(PathSegmentKind::CrateKw),
                SyntaxKind::SELF_TYPE_KW => Some(PathSegmentKind::SelfTypeKw),
                _ => None,
            })
    }

    pub fn name_ref(&self) -> Option<NameRef> {
        child(&self.0)
    }

    pub fn generic_arg_list(&self) -> Option<GenericArgList> {
        child(&self.0)
    }
}

impl GenericArgList {
    pub fn generic_args(&self) -> AstChildren<GenericArg> {
        children(&self.0)
    }

    /// The `::` of a turbofish.
    pub fn coloncolon_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::COLON2)
    }
}

impl TypeArg {
    pub fn ty(&self) -> Option<Type> {
        child(&self.0)
    }
}

impl ConstArg {
    /// A `LITERAL`, a `PREFIX_EXPR` (`-` literal) or a `BLOCK_EXPR`.
    pub fn expr(&self) -> Option<Expr> {
        child(&self.0)
    }
}
