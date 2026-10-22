//! Types.

use super::Parser;
use super::paths::PathMode;
use crate::cst::SyntaxKind::{self, *};

/// What a `PARAM_LIST` holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ParamMode {
    /// `pat: Type` (and a self parameter): functions.
    Fn,
    /// Bare types: function-pointer and closure types.
    Type,
}

impl Parser<'_> {
    pub(super) fn at_type_start(&self) -> bool {
        match self.current() {
            Some(
                IDENT | SELF_TYPE_KW | SELF_KW | SUPER_KW | CRATE_KW | L_PAREN | L_BRACK | AMP
                | UNDERSCORE | FN_KW,
            ) => true,
            Some(CONST_KW) => self.nth_at(1, FN_KW) || self.at_closure_type(1),
            _ => false,
        }
    }

    /// `Fn(` at the `n`-th token.
    fn at_closure_type(&self, n: usize) -> bool {
        self.at_contextual(n, "Fn") && self.nth_at(n.saturating_add(1), L_PAREN)
    }

    pub(super) fn ty(&mut self) {
        match self.current() {
            Some(L_PAREN) => {
                let cp = self.checkpoint();
                self.bump();
                if self.eat(R_PAREN) {
                    self.start_at(cp, TUPLE_TYPE);
                    self.finish();
                    return;
                }
                self.ty();
                let kind = if self.at(COMMA) {
                    while self.eat(COMMA) {
                        if !self.at_type_start() {
                            break;
                        }
                        self.ty();
                    }
                    TUPLE_TYPE
                } else {
                    PAREN_TYPE
                };
                self.expect(R_PAREN);
                self.start_at(cp, kind);
                self.finish();
            }
            Some(L_BRACK) => {
                self.start(ARRAY_TYPE);
                self.bump();
                self.ty();
                if self.expect(SEMICOLON) {
                    self.expr();
                }
                self.expect(R_BRACK);
                self.finish();
            }
            Some(AMP) => {
                self.start(REF_TYPE);
                self.bump();
                self.eat(MUT_KW);
                self.ty();
                self.finish();
            }
            Some(UNDERSCORE) => {
                self.start(INFER_TYPE);
                self.bump();
                self.finish();
            }
            Some(FN_KW) => self.fn_ptr_type(None),
            Some(CONST_KW) if self.nth_at(1, FN_KW) => self.fn_ptr_type(Some(CONST_KW)),
            Some(CONST_KW) if self.at_closure_type(1) => self.closure_type(Some(CONST_KW)),
            Some(IDENT) if self.at_contextual(0, "pushdown") && self.nth_at(1, FN_KW) => {
                self.fn_ptr_type(Some(PUSHDOWN_KW));
            }
            Some(IDENT) if self.at_contextual(0, "scene") && self.nth_at(1, FN_KW) => {
                self.fn_ptr_type(Some(SCENE_KW));
            }
            Some(IDENT) if self.at_contextual(0, "pushdown") && self.at_closure_type(1) => {
                self.closure_type(Some(PUSHDOWN_KW));
            }
            Some(IDENT) if self.at_closure_type(0) => self.closure_type(None),
            Some(IDENT | SELF_KW | SUPER_KW | CRATE_KW | SELF_TYPE_KW) => {
                self.start(PATH_TYPE);
                self.path(PathMode::Type);
                self.finish();
            }
            _ => self.err_missing_node(
                "expected a type",
                &[
                    R_PAREN, R_CURLY, R_BRACK, R_ANGLE, L_CURLY, COMMA, SEMICOLON, EQ, PIPE,
                ],
            ),
        }
    }

    fn fn_kind_keyword(&mut self, keyword: Option<SyntaxKind>) {
        match keyword {
            Some(CONST_KW) => self.bump(),
            Some(kind) => self.bump_remap(kind),
            None => {}
        }
    }

    fn fn_ptr_type(&mut self, keyword: Option<SyntaxKind>) {
        self.start(FN_PTR_TYPE);
        self.fn_kind_keyword(keyword);
        self.bump();
        self.type_params_and_ret();
        self.finish();
    }

    fn closure_type(&mut self, keyword: Option<SyntaxKind>) {
        self.start(CLOSURE_TYPE);
        self.fn_kind_keyword(keyword);
        self.bump();
        self.type_params_and_ret();
        self.finish();
    }

    fn type_params_and_ret(&mut self) {
        if self.at(L_PAREN) {
            self.param_list(ParamMode::Type);
        } else {
            self.error_missing("expected `(`");
        }
        if self.at(THIN_ARROW) {
            self.ret_type();
        }
    }
}
