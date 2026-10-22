//! Forms: `t![…]` text and the comma-separated forms `name!(…)`, `name![…]`, `name!{…}`.
//!
//! The parser accepts any name, delimiter, order and count; the validator checks them.

use super::Parser;
use super::exprs::Done;
use crate::cst::SyntaxKind::*;

impl Parser<'_> {
    /// `IDENT ! (` / `[` / `{`, with the `!` not glued into `!=`.
    pub(super) fn at_form_start(&self) -> bool {
        self.nth_at(0, IDENT)
            && self.nth_at(1, BANG)
            && !self.nth_at(1, NEQ)
            && matches!(self.nth(2), Some(L_PAREN | L_BRACK | L_CURLY))
    }

    pub(super) fn form(&mut self) -> Done {
        if self.nth_text(0) == "t" && self.nth_at(2, L_BRACK) {
            self.text_form();
            return Done::plain(TEXT_FORM);
        }
        let brace = self.nth_at(2, L_CURLY);
        self.form_expr();
        Done {
            kind: FORM_EXPR,
            block_like: brace,
        }
    }

    fn text_form(&mut self) {
        self.start(TEXT_FORM);
        self.name_ref();
        self.bump();
        self.bump();
        loop {
            match self.current() {
                None | Some(R_BRACK | R_PAREN | R_CURLY | SEMICOLON) => break,
                Some(STRING) => self.text_string(),
                Some(L_BRACK) => self.text_ruby(),
                Some(BANG) if self.nth_at(1, L_BRACK) => self.text_effect(),
                Some(L_CURLY) => self.text_interp(),
                Some(COMMA) => {
                    self.err_bump("items of `t![…]` are juxtaposed, not separated by commas");
                }
                Some(_) => {
                    self.err_bump("expected text, ruby `[…](…)`, an effect `![…](…)` or `{…}`")
                }
            }
        }
        self.expect(R_BRACK);
        self.finish();
    }

    fn text_string(&mut self) {
        self.start(TEXT_STRING);
        self.bump();
        self.finish();
    }

    fn text_ruby(&mut self) {
        self.start(TEXT_RUBY);
        self.bump();
        if self.at(STRING) {
            self.bump();
        } else {
            self.error_missing("expected a string literal");
        }
        self.expect(R_BRACK);
        if self.expect(L_PAREN) {
            self.start(RUBY_READINGS);
            if self.at(STRING) {
                self.bump();
                while self.at(PIPE) && !self.at(PIPE2) {
                    self.bump();
                    if self.at(STRING) {
                        self.bump();
                    } else {
                        self.error_missing("expected a string literal");
                        break;
                    }
                }
            } else {
                self.error_missing("expected a string literal");
            }
            self.finish();
            self.expect(R_PAREN);
        }
        self.finish();
    }

    fn text_effect(&mut self) {
        self.start(TEXT_EFFECT);
        self.bump();
        self.bump();
        match self.current() {
            Some(STRING) => self.text_string(),
            Some(L_BRACK) => self.text_ruby(),
            _ => self.error_missing("expected a string literal or a ruby `[…](…)`"),
        }
        self.expect(R_BRACK);
        if self.expect(L_PAREN) {
            self.expr();
            self.expect(R_PAREN);
        }
        self.finish();
    }

    fn text_interp(&mut self) {
        self.start(TEXT_INTERP);
        self.bump();
        self.expr();
        self.expect(R_CURLY);
        self.finish();
    }

    fn at_form_attr(&self) -> bool {
        self.nth_at(0, IDENT)
            && self.nth_at(1, EQ)
            && !self.nth_at(1, EQ2)
            && !self.nth_at(1, FAT_ARROW)
    }

    fn form_expr(&mut self) {
        self.start(FORM_EXPR);
        self.name_ref();
        self.bump();
        let close = match self.current() {
            Some(L_BRACK) => R_BRACK,
            Some(L_CURLY) => R_CURLY,
            _ => R_PAREN,
        };
        self.bump();
        loop {
            if self.current().is_none() || self.at(close) {
                break;
            }
            if self.at_form_attr() {
                self.start(FORM_ATTR);
                self.name();
                self.bump();
                self.expr();
                self.finish();
            } else if self.at_expr_start() {
                let cp = self.checkpoint();
                self.expr();
                if self.at(IF_KW) || self.at(FAT_ARROW) {
                    self.start_at(cp, FORM_ARM);
                    if self.at(IF_KW) {
                        self.start(FORM_GUARD);
                        self.bump();
                        self.expr();
                        self.finish();
                    }
                    if self.expect(FAT_ARROW) {
                        self.expr();
                    }
                    self.finish();
                }
            } else if matches!(
                self.current(),
                Some(R_PAREN | R_BRACK | R_CURLY | SEMICOLON)
            ) {
                break;
            } else {
                self.err_bump("expected an attribute or an element");
                continue;
            }
            if !self.list_comma(|p| p.at_form_attr() || p.at_expr_start()) {
                break;
            }
        }
        self.expect(close);
        self.finish();
    }
}
