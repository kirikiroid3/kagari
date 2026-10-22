//! Statements and expressions.

use rowan::ast::{AstChildren, AstNode};

use super::{
    FormExpr, GenericArgList, Item, Literal, NameRef, ParamList, Pat, Path, RetType, TextForm,
    Type, child, child_after, child_after_token, child_before, children, first_token, token,
};
use crate::cst::{KagariLanguage, SyntaxKind, SyntaxNode, SyntaxToken};

ast_node!(BlockExpr, BLOCK_EXPR);
ast_node!(LetStmt, LET_STMT);
ast_node!(ExprStmt, EXPR_STMT);
ast_node!(PathExpr, PATH_EXPR);
ast_node!(ParenExpr, PAREN_EXPR);
ast_node!(TupleExpr, TUPLE_EXPR);
ast_node!(ArrayExpr, ARRAY_EXPR);
ast_node!(RecordExpr, RECORD_EXPR);
ast_node!(RecordExprFieldList, RECORD_EXPR_FIELD_LIST);
ast_node!(RecordExprField, RECORD_EXPR_FIELD);
ast_node!(IfExpr, IF_EXPR);
ast_node!(LetExpr, LET_EXPR);
ast_node!(MatchExpr, MATCH_EXPR);
ast_node!(MatchArmList, MATCH_ARM_LIST);
ast_node!(MatchArm, MATCH_ARM);
ast_node!(MatchGuard, MATCH_GUARD);
ast_node!(LoopExpr, LOOP_EXPR);
ast_node!(WhileExpr, WHILE_EXPR);
ast_node!(ForExpr, FOR_EXPR);
ast_node!(LabelDecl, LABEL_DECL);
ast_node!(BreakExpr, BREAK_EXPR);
ast_node!(ContinueExpr, CONTINUE_EXPR);
ast_node!(ReturnExpr, RETURN_EXPR);
ast_node!(CallExpr, CALL_EXPR);
ast_node!(ArgList, ARG_LIST);
ast_node!(MethodCallExpr, METHOD_CALL_EXPR);
ast_node!(FieldExpr, FIELD_EXPR);
ast_node!(IndexExpr, INDEX_EXPR);
ast_node!(TryExpr, TRY_EXPR);
ast_node!(CastExpr, CAST_EXPR);
ast_node!(PrefixExpr, PREFIX_EXPR);
ast_node!(RefExpr, REF_EXPR);
ast_node!(BinExpr, BIN_EXPR);
ast_node!(RangeExpr, RANGE_EXPR);
ast_node!(ClosureExpr, CLOSURE_EXPR);

ast_enum!(
    /// Any expression, forms included.
    Expr {
        Literal(LITERAL),
        PathExpr(PATH_EXPR),
        ParenExpr(PAREN_EXPR),
        TupleExpr(TUPLE_EXPR),
        ArrayExpr(ARRAY_EXPR),
        RecordExpr(RECORD_EXPR),
        BlockExpr(BLOCK_EXPR),
        IfExpr(IF_EXPR),
        MatchExpr(MATCH_EXPR),
        LoopExpr(LOOP_EXPR),
        WhileExpr(WHILE_EXPR),
        ForExpr(FOR_EXPR),
        BreakExpr(BREAK_EXPR),
        ContinueExpr(CONTINUE_EXPR),
        ReturnExpr(RETURN_EXPR),
        CallExpr(CALL_EXPR),
        MethodCallExpr(METHOD_CALL_EXPR),
        FieldExpr(FIELD_EXPR),
        IndexExpr(INDEX_EXPR),
        TryExpr(TRY_EXPR),
        CastExpr(CAST_EXPR),
        PrefixExpr(PREFIX_EXPR),
        RefExpr(REF_EXPR),
        BinExpr(BIN_EXPR),
        RangeExpr(RANGE_EXPR),
        ClosureExpr(CLOSURE_EXPR),
        FormExpr(FORM_EXPR),
        TextForm(TEXT_FORM),
    }
);

/// A statement of a block. The tail expression is not a statement.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Stmt {
    LetStmt(LetStmt),
    ExprStmt(ExprStmt),
    Item(Item),
}

impl AstNode for Stmt {
    type Language = KagariLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(kind, SyntaxKind::LET_STMT | SyntaxKind::EXPR_STMT) || Item::can_cast(kind)
    }

    fn cast(node: SyntaxNode) -> Option<Self> {
        Some(match node.kind() {
            SyntaxKind::LET_STMT => Stmt::LetStmt(LetStmt(node)),
            SyntaxKind::EXPR_STMT => Stmt::ExprStmt(ExprStmt(node)),
            _ => Stmt::Item(Item::cast(node)?),
        })
    }

    fn syntax(&self) -> &SyntaxNode {
        match self {
            Stmt::LetStmt(it) => it.syntax(),
            Stmt::ExprStmt(it) => it.syntax(),
            Stmt::Item(it) => it.syntax(),
        }
    }
}

/// The condition of an `if` or `while`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Condition {
    Expr(Expr),
    LetExpr(LetExpr),
}

impl AstNode for Condition {
    type Language = KagariLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LET_EXPR || Expr::can_cast(kind)
    }

    fn cast(node: SyntaxNode) -> Option<Self> {
        if node.kind() == SyntaxKind::LET_EXPR {
            Some(Condition::LetExpr(LetExpr(node)))
        } else {
            Expr::cast(node).map(Condition::Expr)
        }
    }

    fn syntax(&self) -> &SyntaxNode {
        match self {
            Condition::Expr(it) => it.syntax(),
            Condition::LetExpr(it) => it.syntax(),
        }
    }
}

/// What follows `else`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ElseBranch {
    Block(BlockExpr),
    If(IfExpr),
}

/// The field named by a field expression.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FieldName {
    Name(NameRef),
    /// A tuple index (`t.0`).
    Index(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    /// `-`
    Neg,
    /// `!`
    Not,
    /// `*`
    Deref,
}

/// Arithmetic, bitwise and shift operators (also the operator of a compound assignment).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ArithOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Shl,
    Shr,
    BitAnd,
    BitOr,
    BitXor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LogicOp {
    And,
    Or,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BinOp {
    Arith(ArithOp),
    Cmp(CmpOp),
    Logic(LogicOp),
    /// `=`
    Assign,
    /// `+=`, `<<=`, …
    CompoundAssign(ArithOp),
}

impl BinOp {
    pub fn from_kind(kind: SyntaxKind) -> Option<BinOp> {
        use SyntaxKind::*;
        Some(match kind {
            PLUS => BinOp::Arith(ArithOp::Add),
            MINUS => BinOp::Arith(ArithOp::Sub),
            STAR => BinOp::Arith(ArithOp::Mul),
            SLASH => BinOp::Arith(ArithOp::Div),
            PERCENT => BinOp::Arith(ArithOp::Rem),
            SHL => BinOp::Arith(ArithOp::Shl),
            SHR => BinOp::Arith(ArithOp::Shr),
            AMP => BinOp::Arith(ArithOp::BitAnd),
            PIPE => BinOp::Arith(ArithOp::BitOr),
            CARET => BinOp::Arith(ArithOp::BitXor),
            EQ2 => BinOp::Cmp(CmpOp::Eq),
            NEQ => BinOp::Cmp(CmpOp::Ne),
            L_ANGLE => BinOp::Cmp(CmpOp::Lt),
            LTEQ => BinOp::Cmp(CmpOp::Le),
            R_ANGLE => BinOp::Cmp(CmpOp::Gt),
            GTEQ => BinOp::Cmp(CmpOp::Ge),
            AMP2 => BinOp::Logic(LogicOp::And),
            PIPE2 => BinOp::Logic(LogicOp::Or),
            EQ => BinOp::Assign,
            PLUS_EQ => BinOp::CompoundAssign(ArithOp::Add),
            MINUS_EQ => BinOp::CompoundAssign(ArithOp::Sub),
            STAR_EQ => BinOp::CompoundAssign(ArithOp::Mul),
            SLASH_EQ => BinOp::CompoundAssign(ArithOp::Div),
            PERCENT_EQ => BinOp::CompoundAssign(ArithOp::Rem),
            SHL_EQ => BinOp::CompoundAssign(ArithOp::Shl),
            SHR_EQ => BinOp::CompoundAssign(ArithOp::Shr),
            AMP_EQ => BinOp::CompoundAssign(ArithOp::BitAnd),
            PIPE_EQ => BinOp::CompoundAssign(ArithOp::BitOr),
            CARET_EQ => BinOp::CompoundAssign(ArithOp::BitXor),
            _ => return None,
        })
    }
}

/// The two shapes of an array expression.
#[derive(Clone, Debug)]
pub enum ArrayExprKind {
    /// `[a, b, c]`
    ElementList(AstChildren<Expr>),
    /// `[value; count]`
    Repeat {
        initializer: Option<Expr>,
        repeat: Option<Expr>,
    },
}

impl BlockExpr {
    pub fn statements(&self) -> AstChildren<Stmt> {
        children(&self.0)
    }

    /// The trailing expression without `;`, a direct child of the block.
    pub fn tail_expr(&self) -> Option<Expr> {
        child(&self.0)
    }
}

impl LetStmt {
    pub fn pat(&self) -> Option<Pat> {
        child(&self.0)
    }

    pub fn ty(&self) -> Option<Type> {
        child(&self.0)
    }

    pub fn initializer(&self) -> Option<Expr> {
        child_after(&self.0, SyntaxKind::EQ)
    }
}

impl ExprStmt {
    pub fn expr(&self) -> Option<Expr> {
        child(&self.0)
    }

    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::SEMICOLON)
    }
}

impl PathExpr {
    pub fn path(&self) -> Option<Path> {
        child(&self.0)
    }
}

impl ParenExpr {
    pub fn expr(&self) -> Option<Expr> {
        child(&self.0)
    }
}

impl TupleExpr {
    pub fn fields(&self) -> AstChildren<Expr> {
        children(&self.0)
    }
}

impl ArrayExpr {
    /// Every element expression; in the repeat form `[x; n]`, the value then the count.
    pub fn exprs(&self) -> AstChildren<Expr> {
        children(&self.0)
    }

    pub fn semicolon_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::SEMICOLON)
    }

    /// `(value, count)` of `[value; count]` when both are present.
    pub fn repeat(&self) -> Option<(Expr, Expr)> {
        match self.kind() {
            ArrayExprKind::Repeat {
                initializer: Some(value),
                repeat: Some(count),
            } => Some((value, count)),
            _ => None,
        }
    }

    pub fn kind(&self) -> ArrayExprKind {
        if self.semicolon_token().is_some() {
            ArrayExprKind::Repeat {
                initializer: child_before(&self.0, SyntaxKind::SEMICOLON),
                repeat: child_after(&self.0, SyntaxKind::SEMICOLON),
            }
        } else {
            ArrayExprKind::ElementList(self.exprs())
        }
    }
}

impl RecordExpr {
    pub fn path(&self) -> Option<Path> {
        child(&self.0)
    }

    pub fn record_expr_field_list(&self) -> Option<RecordExprFieldList> {
        child(&self.0)
    }
}

impl RecordExprFieldList {
    pub fn fields(&self) -> AstChildren<RecordExprField> {
        children(&self.0)
    }
}

impl RecordExprField {
    pub fn name_ref(&self) -> Option<NameRef> {
        child(&self.0)
    }

    /// The value of `name: expr`; `None` for the shorthand `name`.
    pub fn expr(&self) -> Option<Expr> {
        child(&self.0)
    }
}

impl IfExpr {
    /// The first child: an expression or `let PAT = EXPR`.
    pub fn condition(&self) -> Option<Condition> {
        self.0.first_child().and_then(Condition::cast)
    }

    pub fn then_branch(&self) -> Option<BlockExpr> {
        self.0
            .children()
            .take_while(|it| !self.is_after_else(it))
            .skip(1)
            .find_map(BlockExpr::cast)
    }

    pub fn else_branch(&self) -> Option<ElseBranch> {
        let node = child_after_token::<Expr>(&self.0, |t| t.kind() == SyntaxKind::ELSE_KW)?;
        match node {
            Expr::BlockExpr(block) => Some(ElseBranch::Block(block)),
            Expr::IfExpr(if_expr) => Some(ElseBranch::If(if_expr)),
            _ => None,
        }
    }

    pub fn else_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::ELSE_KW)
    }

    fn is_after_else(&self, node: &SyntaxNode) -> bool {
        self.else_token()
            .is_some_and(|t| node.text_range().start() >= t.text_range().end())
    }
}

impl LetExpr {
    pub fn pat(&self) -> Option<Pat> {
        child(&self.0)
    }

    pub fn expr(&self) -> Option<Expr> {
        child_after(&self.0, SyntaxKind::EQ)
    }
}

impl MatchExpr {
    /// The scrutinee.
    pub fn expr(&self) -> Option<Expr> {
        child(&self.0)
    }

    pub fn match_arm_list(&self) -> Option<MatchArmList> {
        child(&self.0)
    }
}

impl MatchArmList {
    pub fn arms(&self) -> AstChildren<MatchArm> {
        children(&self.0)
    }
}

impl MatchArm {
    pub fn pat(&self) -> Option<Pat> {
        child(&self.0)
    }

    pub fn guard(&self) -> Option<MatchGuard> {
        child(&self.0)
    }

    /// The arm body after `=>`.
    pub fn expr(&self) -> Option<Expr> {
        child_after(&self.0, SyntaxKind::FAT_ARROW)
    }
}

impl MatchGuard {
    pub fn condition(&self) -> Option<Expr> {
        child(&self.0)
    }
}

impl LoopExpr {
    pub fn label_decl(&self) -> Option<LabelDecl> {
        child(&self.0)
    }

    pub fn loop_body(&self) -> Option<BlockExpr> {
        child(&self.0)
    }
}

impl WhileExpr {
    pub fn label_decl(&self) -> Option<LabelDecl> {
        child(&self.0)
    }

    pub fn condition(&self) -> Option<Condition> {
        child_after(&self.0, SyntaxKind::WHILE_KW)
    }

    /// The block after the condition.
    pub fn loop_body(&self) -> Option<BlockExpr> {
        self.0
            .children()
            .skip_while(|it| it.kind() == SyntaxKind::LABEL_DECL)
            .skip(1)
            .find_map(BlockExpr::cast)
    }
}

impl ForExpr {
    pub fn label_decl(&self) -> Option<LabelDecl> {
        child(&self.0)
    }

    pub fn pat(&self) -> Option<Pat> {
        child(&self.0)
    }

    pub fn iterable(&self) -> Option<Expr> {
        child_after(&self.0, SyntaxKind::IN_KW)
    }

    /// The block after the iterable.
    pub fn loop_body(&self) -> Option<BlockExpr> {
        self.0
            .children_with_tokens()
            .skip_while(|it| it.kind() != SyntaxKind::IN_KW)
            .filter_map(|it| it.into_node())
            .skip(1)
            .find_map(BlockExpr::cast)
    }
}

impl LabelDecl {
    pub fn label_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::LABEL)
    }
}

impl BreakExpr {
    pub fn label_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::LABEL)
    }

    pub fn expr(&self) -> Option<Expr> {
        child(&self.0)
    }
}

impl ContinueExpr {
    pub fn label_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::LABEL)
    }
}

impl ReturnExpr {
    pub fn expr(&self) -> Option<Expr> {
        child(&self.0)
    }
}

impl CallExpr {
    /// The callee.
    pub fn expr(&self) -> Option<Expr> {
        child(&self.0)
    }

    pub fn arg_list(&self) -> Option<ArgList> {
        child(&self.0)
    }
}

impl ArgList {
    pub fn args(&self) -> AstChildren<Expr> {
        children(&self.0)
    }
}

impl MethodCallExpr {
    pub fn receiver(&self) -> Option<Expr> {
        child(&self.0)
    }

    pub fn name_ref(&self) -> Option<NameRef> {
        child(&self.0)
    }

    pub fn generic_arg_list(&self) -> Option<GenericArgList> {
        child(&self.0)
    }

    pub fn arg_list(&self) -> Option<ArgList> {
        child(&self.0)
    }
}

impl FieldExpr {
    pub fn base(&self) -> Option<Expr> {
        child(&self.0)
    }

    pub fn field(&self) -> Option<FieldName> {
        if let Some(name_ref) = child::<NameRef>(&self.0) {
            return Some(FieldName::Name(name_ref));
        }
        let index = token(&self.0, SyntaxKind::INT_NUMBER)?;
        index.text().parse().ok().map(FieldName::Index)
    }
}

impl IndexExpr {
    pub fn base(&self) -> Option<Expr> {
        child_before(&self.0, SyntaxKind::L_BRACK)
    }

    pub fn index(&self) -> Option<Expr> {
        child_after(&self.0, SyntaxKind::L_BRACK)
    }
}

impl TryExpr {
    pub fn expr(&self) -> Option<Expr> {
        child(&self.0)
    }
}

impl CastExpr {
    pub fn expr(&self) -> Option<Expr> {
        child(&self.0)
    }

    pub fn ty(&self) -> Option<Type> {
        child(&self.0)
    }
}

impl PrefixExpr {
    pub fn op_token(&self) -> Option<SyntaxToken> {
        first_token(&self.0)
    }

    pub fn op_kind(&self) -> Option<UnaryOp> {
        match self.op_token()?.kind() {
            SyntaxKind::MINUS => Some(UnaryOp::Neg),
            SyntaxKind::BANG => Some(UnaryOp::Not),
            SyntaxKind::STAR => Some(UnaryOp::Deref),
            _ => None,
        }
    }

    pub fn expr(&self) -> Option<Expr> {
        child(&self.0)
    }
}

impl RefExpr {
    pub fn amp_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::AMP)
    }

    pub fn mut_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::MUT_KW)
    }

    pub fn expr(&self) -> Option<Expr> {
        child(&self.0)
    }
}

impl BinExpr {
    /// The operator: the only direct non-trivia token.
    pub fn op_token(&self) -> Option<SyntaxToken> {
        first_token(&self.0)
    }

    pub fn op_kind(&self) -> Option<BinOp> {
        BinOp::from_kind(self.op_token()?.kind())
    }

    pub fn lhs(&self) -> Option<Expr> {
        let op = self.op_token()?;
        self.0
            .children()
            .take_while(|it| it.text_range().end() <= op.text_range().start())
            .find_map(Expr::cast)
    }

    pub fn rhs(&self) -> Option<Expr> {
        let op = self.op_token()?;
        self.0
            .children()
            .filter(|it| it.text_range().start() >= op.text_range().end())
            .find_map(Expr::cast)
    }
}

impl RangeExpr {
    /// The `..` or `..=` token.
    pub fn op_token(&self) -> Option<SyntaxToken> {
        first_token(&self.0)
    }

    pub fn start(&self) -> Option<Expr> {
        let op = self.op_token()?;
        child_before(&self.0, op.kind())
    }

    pub fn end(&self) -> Option<Expr> {
        let op = self.op_token()?;
        child_after(&self.0, op.kind())
    }

    pub fn is_inclusive(&self) -> bool {
        self.op_token()
            .is_some_and(|t| t.kind() == SyntaxKind::DOT2EQ)
    }
}

impl ClosureExpr {
    /// The `|…|` parameters (a `PARAM_LIST` delimited by pipes).
    pub fn param_list(&self) -> Option<ParamList> {
        child(&self.0)
    }

    pub fn ret_type(&self) -> Option<RetType> {
        child(&self.0)
    }

    pub fn body(&self) -> Option<Expr> {
        child(&self.0)
    }
}
