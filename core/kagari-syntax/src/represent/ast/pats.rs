//! Patterns.

use rowan::ast::{AstChildren, AstNode, support};

use super::{Literal, Name, NameRef, Path, tokens};
use crate::represent::cst::{RepresentKind, SyntaxToken};

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
