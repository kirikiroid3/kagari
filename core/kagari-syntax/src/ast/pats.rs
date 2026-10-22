//! Patterns.

use rowan::ast::AstChildren;

use super::{Literal, NameRef, Path, child, child_before, children, first_token, token};
use crate::cst::{SyntaxKind, SyntaxToken};

ast_node!(WildcardPat, WILDCARD_PAT);
ast_node!(IdentPat, IDENT_PAT);
ast_node!(PathPat, PATH_PAT);
ast_node!(LiteralPat, LITERAL_PAT);
ast_node!(RangePat, RANGE_PAT);
ast_node!(TuplePat, TUPLE_PAT);
ast_node!(TupleStructPat, TUPLE_STRUCT_PAT);
ast_node!(RecordPat, RECORD_PAT);
ast_node!(RecordPatFieldList, RECORD_PAT_FIELD_LIST);
ast_node!(RecordPatField, RECORD_PAT_FIELD);
ast_node!(RestPat, REST_PAT);
ast_node!(OrPat, OR_PAT);
ast_node!(ParenPat, PAREN_PAT);

ast_enum!(Pat {
    WildcardPat(WILDCARD_PAT),
    IdentPat(IDENT_PAT),
    PathPat(PATH_PAT),
    LiteralPat(LITERAL_PAT),
    RangePat(RANGE_PAT),
    TuplePat(TUPLE_PAT),
    TupleStructPat(TUPLE_STRUCT_PAT),
    RecordPat(RECORD_PAT),
    RestPat(REST_PAT),
    OrPat(OR_PAT),
    ParenPat(PAREN_PAT),
});

impl IdentPat {
    pub fn mut_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::MUT_KW)
    }
}

impl PathPat {
    pub fn path(&self) -> Option<Path> {
        child(&self.0)
    }
}

impl LiteralPat {
    pub fn minus_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::MINUS)
    }

    pub fn literal(&self) -> Option<Literal> {
        child(&self.0)
    }
}

/// The `..` or `..=` token of a range.
fn range_op(node: &crate::cst::SyntaxNode) -> Option<SyntaxToken> {
    first_token(node).filter(|t| matches!(t.kind(), SyntaxKind::DOT2 | SyntaxKind::DOT2EQ))
}

impl RangePat {
    pub fn op_token(&self) -> Option<SyntaxToken> {
        range_op(&self.0)
    }

    /// The bound before the operator, if any.
    pub fn start(&self) -> Option<super::Pat> {
        let op = self.op_token()?;
        child_before(&self.0, op.kind())
    }

    /// The bound after the operator, if any.
    pub fn end(&self) -> Option<super::Pat> {
        let op = self.op_token()?;
        super::child_after(&self.0, op.kind())
    }

    pub fn is_inclusive(&self) -> bool {
        self.op_token()
            .is_some_and(|t| t.kind() == SyntaxKind::DOT2EQ)
    }
}

impl TuplePat {
    pub fn fields(&self) -> AstChildren<Pat> {
        children(&self.0)
    }
}

impl TupleStructPat {
    pub fn path(&self) -> Option<Path> {
        child(&self.0)
    }

    pub fn fields(&self) -> AstChildren<Pat> {
        children(&self.0)
    }
}

impl RecordPat {
    pub fn path(&self) -> Option<Path> {
        child(&self.0)
    }

    pub fn record_pat_field_list(&self) -> Option<RecordPatFieldList> {
        child(&self.0)
    }
}

impl RecordPatFieldList {
    pub fn fields(&self) -> AstChildren<RecordPatField> {
        children(&self.0)
    }

    /// A trailing `..`.
    pub fn rest_pat(&self) -> Option<RestPat> {
        child(&self.0)
    }
}

impl RecordPatField {
    /// The field name of `name: pat`; `None` for the shorthand `[mut] name`.
    pub fn name_ref(&self) -> Option<NameRef> {
        child(&self.0)
    }

    /// The pattern of `name: pat`, or the `IDENT_PAT` of the shorthand.
    pub fn pat(&self) -> Option<Pat> {
        child(&self.0)
    }
}

impl OrPat {
    pub fn pats(&self) -> AstChildren<Pat> {
        children(&self.0)
    }
}

impl ParenPat {
    pub fn pat(&self) -> Option<Pat> {
        child(&self.0)
    }
}
