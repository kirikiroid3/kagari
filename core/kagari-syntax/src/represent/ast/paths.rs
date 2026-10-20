//! Names, paths and types.

use rowan::ast::{AstChildren, support};

use super::{GenericArgList, token_text};
use crate::represent::cst::{RepresentKind, SyntaxToken};

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
