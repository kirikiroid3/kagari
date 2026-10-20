//! Expressions.

use rowan::ast::{AstChildren, AstNode, support};

use super::{BlockExpr, GenericArgList, Literal, NameRef, Pat, Path, Type, tokens};
use crate::represent::cst::{RepresentKind, SyntaxToken};

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
