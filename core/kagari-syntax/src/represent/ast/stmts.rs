//! Blocks and statements.

use rowan::ast::{AstChildren, support};

use super::{Expr, Pat, Type};
use crate::represent::cst::{RepresentKind, SyntaxToken};

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
