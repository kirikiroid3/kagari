//! Statements and expressions.

use super::Parser;
use super::paths::PathMode;
use crate::cst::SyntaxKind::{self, *};

/// Context restrictions of an expression.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Restrictions {
    /// `PATH {` is not a record literal (conditions, scrutinees, iterables).
    pub(super) no_struct: bool,
    /// Statement position: a block-like expression ends the statement.
    pub(super) prefer_stmt: bool,
}

impl Restrictions {
    fn operand(self) -> Restrictions {
        Restrictions {
            no_struct: self.no_struct,
            prefer_stmt: false,
        }
    }
}

const NO_STRUCT: Restrictions = Restrictions {
    no_struct: true,
    prefer_stmt: false,
};

/// The outermost node an expression parse built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Done {
    pub(super) kind: SyntaxKind,
    /// Block, `if`, `match`, loops and `name!{…}` forms: no `;` needed as a statement.
    pub(super) block_like: bool,
}

impl Done {
    pub(super) fn plain(kind: SyntaxKind) -> Done {
        Done {
            kind,
            block_like: false,
        }
    }

    fn block(kind: SyntaxKind) -> Done {
        Done {
            kind,
            block_like: true,
        }
    }
}

const BP_ASSIGN: u8 = 1;
const BP_RANGE: u8 = 3;
const BP_CAST: u8 = 23;
const BP_PREFIX: u8 = 25;

/// Composite operators, longest first, so that `>>=` wins over `>>` and `>`.
const COMPOSITE_OPS: &[SyntaxKind] = &[
    SHL_EQ, SHR_EQ, DOT2EQ, FAT_ARROW, THIN_ARROW, EQ2, NEQ, LTEQ, GTEQ, AMP2, PIPE2, SHL, SHR,
    PLUS_EQ, MINUS_EQ, STAR_EQ, SLASH_EQ, PERCENT_EQ, CARET_EQ, AMP_EQ, PIPE_EQ, DOT2,
];

/// `(left, right)` binding powers of a binary operator.
fn infix_bp(kind: SyntaxKind) -> Option<(u8, u8)> {
    Some(match kind {
        EQ | PLUS_EQ | MINUS_EQ | STAR_EQ | SLASH_EQ | PERCENT_EQ | CARET_EQ | AMP_EQ | PIPE_EQ
        | SHL_EQ | SHR_EQ => (BP_ASSIGN, BP_ASSIGN),
        DOT2 | DOT2EQ => (BP_RANGE, BP_RANGE.saturating_add(1)),
        PIPE2 => (5, 6),
        AMP2 => (7, 8),
        EQ2 | NEQ | L_ANGLE | R_ANGLE | LTEQ | GTEQ => (9, 10),
        PIPE => (11, 12),
        CARET => (13, 14),
        AMP => (15, 16),
        SHL | SHR => (17, 18),
        PLUS | MINUS => (19, 20),
        STAR | SLASH | PERCENT => (21, 22),
        _ => return None,
    })
}

fn is_comparison(kind: SyntaxKind) -> bool {
    matches!(kind, EQ2 | NEQ | L_ANGLE | R_ANGLE | LTEQ | GTEQ)
}

impl Parser<'_> {
    // ---- statements ---------------------------------------------------------------------

    pub(super) fn block_expr(&mut self) {
        self.start(BLOCK_EXPR);
        self.bump();
        self.stmts();
        self.expect(R_CURLY);
        self.finish();
    }

    fn block_or_error(&mut self) {
        if self.at(L_CURLY) {
            self.block_expr();
        } else {
            self.error_missing("expected `{`");
        }
    }

    fn stmts(&mut self) {
        loop {
            match self.current() {
                None | Some(R_CURLY) => break,
                Some(SEMICOLON) => self.bump(),
                Some(LET_KW) => self.let_stmt(),
                Some(_) if self.at_item_start() => self.item(false),
                Some(_) if !self.at_expr_start() => self.err_bump("expected an expression"),
                Some(_) => {
                    let cp = self.checkpoint();
                    let done = self.expr_bp(
                        0,
                        Restrictions {
                            no_struct: false,
                            prefer_stmt: true,
                        },
                    );
                    if self.at(SEMICOLON) {
                        self.start_at(cp, EXPR_STMT);
                        self.bump();
                        self.finish();
                    } else if matches!(self.current(), None | Some(R_CURLY)) {
                        // The tail expression stays a direct child of the block.
                    } else if done.is_some_and(|d| d.block_like) {
                        self.start_at(cp, EXPR_STMT);
                        self.finish();
                    } else {
                        self.start_at(cp, EXPR_STMT);
                        self.error_missing("expected `;`");
                        self.finish();
                        self.recover_stmt();
                    }
                }
            }
        }
    }

    /// After a missing `;`: if the next token cannot start a statement, skip to the next `;`
    /// (consumed) or `}`.
    fn recover_stmt(&mut self) {
        match self.current() {
            None | Some(R_CURLY | LET_KW | SEMICOLON) => return,
            Some(_) if self.at_item_start() || self.at_expr_start() => return,
            Some(_) => {}
        }
        self.start(ERROR);
        loop {
            match self.current() {
                None | Some(R_CURLY) => break,
                Some(SEMICOLON) => {
                    self.bump();
                    break;
                }
                Some(_) => self.bump(),
            }
        }
        self.finish();
    }

    fn let_stmt(&mut self) {
        self.start(LET_STMT);
        self.bump();
        self.pat_top();
        if self.at(COLON) && !self.at(COLON2) {
            self.bump();
            self.ty();
        }
        if self.eat(EQ) {
            self.expr();
        }
        let terminated = self.expect(SEMICOLON);
        self.finish();
        if !terminated {
            self.recover_stmt();
        }
    }

    // ---- expressions --------------------------------------------------------------------

    pub(super) fn at_expr_start(&self) -> bool {
        matches!(
            self.current(),
            Some(
                INT_NUMBER
                    | DECIMAL_NUMBER
                    | STRING
                    | TRUE_KW
                    | FALSE_KW
                    | IDENT
                    | SELF_KW
                    | SUPER_KW
                    | CRATE_KW
                    | SELF_TYPE_KW
                    | L_PAREN
                    | L_BRACK
                    | L_CURLY
                    | MINUS
                    | BANG
                    | STAR
                    | AMP
                    | PIPE
                    | IF_KW
                    | MATCH_KW
                    | LOOP_KW
                    | WHILE_KW
                    | FOR_KW
                    | LABEL
                    | BREAK_KW
                    | CONTINUE_KW
                    | RETURN_KW
            )
        ) || self.at(DOT2)
    }

    /// Whether an operand may start here under `r` (a `{` is not an operand where record
    /// literals are restricted, since it opens the block that follows).
    fn at_operand_start(&self, r: Restrictions) -> bool {
        self.at_expr_start() && !(r.no_struct && self.at(L_CURLY))
    }

    pub(super) fn expr(&mut self) -> Option<Done> {
        self.expr_bp(0, Restrictions::default())
    }

    fn peek_binop(&self) -> Option<(SyntaxKind, u8, u8)> {
        let kind = COMPOSITE_OPS
            .iter()
            .copied()
            .find(|kind| self.at(*kind))
            .or_else(|| self.current())?;
        let (left, right) = infix_bp(kind)?;
        Some((kind, left, right))
    }

    /// Pratt loop. Returns the outermost node built, or `None` if no expression could be
    /// parsed (an `ERROR` node was left instead).
    pub(super) fn expr_bp(&mut self, min_bp: u8, r: Restrictions) -> Option<Done> {
        let cp = self.checkpoint();
        let mut done = self.lhs(r)?;
        if r.prefer_stmt && done.block_like {
            return Some(done);
        }
        let mut after_comparison = false;
        loop {
            if self.at(AS_KW) {
                if BP_CAST < min_bp {
                    break;
                }
                self.start_at(cp, CAST_EXPR);
                self.bump();
                self.ty();
                self.finish();
                done = Done::plain(CAST_EXPR);
                after_comparison = false;
                continue;
            }
            let Some((op, left, right)) = self.peek_binop() else {
                break;
            };
            if left < min_bp {
                break;
            }
            if matches!(op, DOT2 | DOT2EQ) {
                self.start_at(cp, RANGE_EXPR);
                self.bump_composite(op);
                if self.at_operand_start(r) {
                    self.expr_bp(right, r.operand());
                } else if op == DOT2EQ {
                    self.error_missing("expected an expression");
                }
                self.finish();
                done = Done::plain(RANGE_EXPR);
                after_comparison = false;
                continue;
            }
            let comparison = is_comparison(op);
            if comparison && after_comparison {
                self.error("comparison operators cannot be chained");
            }
            self.start_at(cp, BIN_EXPR);
            self.bump_composite(op);
            self.expr_bp(right, r.operand());
            self.finish();
            done = Done::plain(BIN_EXPR);
            after_comparison = comparison;
        }
        Some(done)
    }

    fn lhs(&mut self, r: Restrictions) -> Option<Done> {
        if self.at(DOT2EQ) || self.at(DOT2) {
            let op = if self.at(DOT2EQ) { DOT2EQ } else { DOT2 };
            self.start(RANGE_EXPR);
            self.bump_composite(op);
            if self.at_operand_start(r) {
                self.expr_bp(BP_RANGE.saturating_add(1), r.operand());
            } else if op == DOT2EQ {
                self.error_missing("expected an expression");
            }
            self.finish();
            return Some(Done::plain(RANGE_EXPR));
        }
        match self.current() {
            Some(MINUS | BANG | STAR) => {
                self.start(PREFIX_EXPR);
                self.bump();
                self.expr_bp(BP_PREFIX, r.operand());
                self.finish();
                Some(Done::plain(PREFIX_EXPR))
            }
            Some(AMP) => {
                self.start(REF_EXPR);
                self.bump();
                self.eat(MUT_KW);
                self.expr_bp(BP_PREFIX, r.operand());
                self.finish();
                Some(Done::plain(REF_EXPR))
            }
            _ => self.postfix(r),
        }
    }

    fn postfix(&mut self, r: Restrictions) -> Option<Done> {
        let cp = self.checkpoint();
        let mut done = self.primary(r)?;
        loop {
            let stmt_block = r.prefer_stmt && done.block_like;
            let kind = if self.at(QUESTION) {
                self.start_at(cp, TRY_EXPR);
                self.bump();
                TRY_EXPR
            } else if self.at(DOT) && !self.at(DOT2) {
                let method = self.nth_at(1, IDENT)
                    && (self.nth_at(2, L_PAREN)
                        || (self.nth_at(2, COLON2) && self.nth_at(4, L_ANGLE)));
                let kind = if method { METHOD_CALL_EXPR } else { FIELD_EXPR };
                self.start_at(cp, kind);
                self.bump();
                match self.current() {
                    Some(IDENT) => self.name_ref(),
                    Some(INT_NUMBER) if !method => self.bump(),
                    _ => self.error_missing("expected a field or method name"),
                }
                if method {
                    if self.at(COLON2) {
                        self.generic_arg_list(true);
                    }
                    if self.at(L_PAREN) {
                        self.arg_list();
                    } else {
                        self.error_missing("expected `(`");
                    }
                }
                kind
            } else if self.at(L_PAREN) && !stmt_block {
                self.start_at(cp, CALL_EXPR);
                self.arg_list();
                CALL_EXPR
            } else if self.at(L_BRACK) && !stmt_block {
                self.start_at(cp, INDEX_EXPR);
                self.bump();
                self.expr();
                self.expect(R_BRACK);
                INDEX_EXPR
            } else {
                break;
            };
            self.finish();
            done = Done::plain(kind);
        }
        Some(done)
    }

    fn primary(&mut self, r: Restrictions) -> Option<Done> {
        match self.current() {
            Some(INT_NUMBER | DECIMAL_NUMBER | STRING | TRUE_KW | FALSE_KW) => {
                self.literal();
                Some(Done::plain(LITERAL))
            }
            Some(IDENT) if self.at_form_start() => Some(self.form()),
            Some(IDENT | SELF_KW | SUPER_KW | CRATE_KW | SELF_TYPE_KW) => {
                let cp = self.checkpoint();
                self.path(PathMode::Expr);
                if !r.no_struct && self.at(L_CURLY) {
                    self.start_at(cp, RECORD_EXPR);
                    self.record_expr_field_list();
                    self.finish();
                    Some(Done::plain(RECORD_EXPR))
                } else {
                    self.start_at(cp, PATH_EXPR);
                    self.finish();
                    Some(Done::plain(PATH_EXPR))
                }
            }
            Some(L_PAREN) => {
                let cp = self.checkpoint();
                self.bump();
                if self.eat(R_PAREN) {
                    self.start_at(cp, TUPLE_EXPR);
                    self.finish();
                    return Some(Done::plain(TUPLE_EXPR));
                }
                self.expr();
                let kind = if self.at(COMMA) {
                    while self.eat(COMMA) {
                        if !self.at_expr_start() {
                            break;
                        }
                        self.expr();
                    }
                    TUPLE_EXPR
                } else {
                    PAREN_EXPR
                };
                self.expect(R_PAREN);
                self.start_at(cp, kind);
                self.finish();
                Some(Done::plain(kind))
            }
            Some(L_BRACK) => {
                self.start(ARRAY_EXPR);
                self.bump();
                if !self.at(R_BRACK) {
                    self.expr();
                    if self.eat(SEMICOLON) {
                        self.expr();
                    } else {
                        while self.eat(COMMA) {
                            if !self.at_expr_start() {
                                break;
                            }
                            self.expr();
                        }
                    }
                }
                self.expect(R_BRACK);
                self.finish();
                Some(Done::plain(ARRAY_EXPR))
            }
            Some(L_CURLY) => {
                self.block_expr();
                Some(Done::block(BLOCK_EXPR))
            }
            Some(IF_KW) => {
                self.if_expr();
                Some(Done::block(IF_EXPR))
            }
            Some(MATCH_KW) => {
                self.match_expr();
                Some(Done::block(MATCH_EXPR))
            }
            Some(LOOP_KW | WHILE_KW | FOR_KW) => {
                let cp = self.checkpoint();
                Some(self.loop_like(cp))
            }
            Some(LABEL) => {
                let cp = self.checkpoint();
                self.start(LABEL_DECL);
                self.bump();
                self.expect(COLON);
                self.finish();
                if matches!(self.current(), Some(LOOP_KW | WHILE_KW | FOR_KW)) {
                    Some(self.loop_like(cp))
                } else {
                    self.error("expected `loop`, `while` or `for` after a label");
                    self.start_at(cp, ERROR);
                    self.finish();
                    None
                }
            }
            Some(BREAK_KW) => {
                self.start(BREAK_EXPR);
                self.bump();
                self.eat(LABEL);
                if self.at_operand_start(r) {
                    self.expr_bp(0, r.operand());
                }
                self.finish();
                Some(Done::plain(BREAK_EXPR))
            }
            Some(CONTINUE_KW) => {
                self.start(CONTINUE_EXPR);
                self.bump();
                self.eat(LABEL);
                self.finish();
                Some(Done::plain(CONTINUE_EXPR))
            }
            Some(RETURN_KW) => {
                self.start(RETURN_EXPR);
                self.bump();
                if self.at_operand_start(r) {
                    self.expr_bp(0, r.operand());
                }
                self.finish();
                Some(Done::plain(RETURN_EXPR))
            }
            Some(PIPE) => {
                self.closure_expr(r);
                Some(Done::plain(CLOSURE_EXPR))
            }
            _ => {
                self.err_missing_node(
                    "expected an expression",
                    &[R_PAREN, R_CURLY, R_BRACK, COMMA, SEMICOLON],
                );
                None
            }
        }
    }

    fn closure_expr(&mut self, r: Restrictions) {
        self.start(CLOSURE_EXPR);
        self.start(PARAM_LIST);
        self.bump();
        if self.at(PIPE) {
            self.bump();
        } else {
            while self.at_pat_start() {
                self.start(PARAM);
                self.pat_single();
                if self.at(COLON) && !self.at(COLON2) {
                    self.bump();
                    self.ty();
                }
                self.finish();
                if !self.list_comma(Self::at_pat_start) {
                    break;
                }
            }
            self.expect(PIPE);
        }
        self.finish();
        if self.at(THIN_ARROW) {
            self.ret_type();
            self.block_or_error();
        } else {
            self.expr_bp(0, r.operand());
        }
        self.finish();
    }

    fn record_expr_field_list(&mut self) {
        self.start(RECORD_EXPR_FIELD_LIST);
        self.bump();
        loop {
            match self.current() {
                None | Some(R_CURLY) => break,
                Some(IDENT) => {
                    self.start(RECORD_EXPR_FIELD);
                    self.name_ref();
                    if self.at(COLON) && !self.at(COLON2) {
                        self.bump();
                        self.expr();
                    }
                    self.finish();
                }
                Some(_) if self.at_item_start() => break,
                Some(_) => {
                    self.err_bump("expected a field");
                    continue;
                }
            }
            if !self.list_comma(|p| p.at(IDENT)) {
                break;
            }
        }
        self.expect(R_CURLY);
        self.finish();
    }

    fn arg_list(&mut self) {
        self.start(ARG_LIST);
        self.bump();
        loop {
            match self.current() {
                None | Some(R_PAREN) => break,
                _ => {}
            }
            self.expr();
            if !self.list_comma(Self::at_expr_start) {
                break;
            }
        }
        self.expect(R_PAREN);
        self.finish();
    }

    /// `if`/`while` condition: an expression or `let PAT = EXPR`.
    fn condition(&mut self) {
        if self.at(LET_KW) {
            self.start(LET_EXPR);
            self.bump();
            self.pat_top();
            if self.expect(EQ) {
                self.expr_bp(0, NO_STRUCT);
            }
            self.finish();
        } else {
            self.expr_bp(0, NO_STRUCT);
        }
    }

    fn if_expr(&mut self) {
        self.start(IF_EXPR);
        self.bump();
        self.condition();
        self.block_or_error();
        if self.eat(ELSE_KW) {
            if self.at(IF_KW) {
                self.if_expr();
            } else if self.at(L_CURLY) {
                self.block_expr();
            } else {
                self.error_missing("expected `{` or `if`");
            }
        }
        self.finish();
    }

    /// `loop`, `while` or `for` at the current token, wrapped from `cp` (which may include a
    /// `LABEL_DECL`).
    fn loop_like(&mut self, cp: rowan::Checkpoint) -> Done {
        let kind = match self.current() {
            Some(WHILE_KW) => WHILE_EXPR,
            Some(FOR_KW) => FOR_EXPR,
            _ => LOOP_EXPR,
        };
        self.start_at(cp, kind);
        self.bump();
        match kind {
            WHILE_EXPR => self.condition(),
            FOR_EXPR => {
                self.pat_top();
                if self.expect(IN_KW) {
                    self.expr_bp(0, NO_STRUCT);
                }
            }
            _ => {}
        }
        self.block_or_error();
        self.finish();
        Done::block(kind)
    }

    fn match_expr(&mut self) {
        self.start(MATCH_EXPR);
        self.bump();
        self.expr_bp(0, NO_STRUCT);
        if self.at(L_CURLY) {
            self.match_arm_list();
        } else {
            self.error_missing("expected `{`");
        }
        self.finish();
    }

    fn match_arm_list(&mut self) {
        self.start(MATCH_ARM_LIST);
        self.bump();
        loop {
            match self.current() {
                None | Some(R_CURLY) => break,
                _ => {}
            }
            if !self.at_pat_start() && !self.at(PIPE) {
                if self.at_item_start() {
                    break;
                }
                self.err_bump("expected a match arm");
                continue;
            }
            self.start(MATCH_ARM);
            self.pat_top();
            if self.at(IF_KW) {
                self.start(MATCH_GUARD);
                self.bump();
                self.expr();
                self.finish();
            }
            // Like a statement, a block-like body ends the arm: `A => {} | B => …`.
            let body = if self.expect(FAT_ARROW) {
                self.expr_bp(
                    0,
                    Restrictions {
                        no_struct: false,
                        prefer_stmt: true,
                    },
                )
            } else {
                None
            };
            if !self.eat(COMMA) && !self.at(R_CURLY) && !body.is_some_and(|d| d.block_like) {
                self.error_missing("expected `,`");
            }
            self.finish();
        }
        self.expect(R_CURLY);
        self.finish();
    }
}
